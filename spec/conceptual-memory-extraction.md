# Conceptual Memory Extraction

## Overview

CCMemory extracts **conceptual insights** from Claude Code sessions, not verbose logs. The goal is to capture knowledge that would be useful in **future sessions**: user preferences, codebase understanding, architectural decisions, gotchas, and patterns.

This is fundamentally different from logging tool usage. A 2-hour session with 500 tool calls might produce only 5-10 valuable memories.

## Design Principles

1. **Conceptual, not granular**: Extract insights, not events
2. **User corrections are gold**: When user corrects Claude, that's highest-value memory
3. **Quality over quantity**: Few valuable memories > many noisy logs
4. **Natural boundaries**: Extract at conversation turns and compaction, not per-tool
5. **No injection**: Memories are available for search, not auto-injected at session start
6. **Superseding**: New information can supersede old, with inference to detect conflicts

## Memory Types

### `preference`
User preferences and corrections. Highest value.

```
"User prefers tabs over spaces"
"Don't use `any` types - use `unknown` with type guards"
"Always add error handling to async functions"
"Use Vitest, not Jest"
```

**Signals in user prompts:**
- "no, don't...", "not like that", "wrong"
- "I prefer...", "always use...", "never use..."
- "instead of X, use Y"

### `codebase`
How the codebase works. Architecture, structure, conventions.

```
"Authentication is in src/auth/ using JWT with httpOnly cookies"
"Database uses libSQL with Drizzle ORM, migrations in db/migrations/"
"Build command is `bun run build`, requires `bun run prebuild` first"
"Tests require DATABASE_URL environment variable"
```

### `decision`
Architectural or design decisions with rationale.

```
"Chose libSQL over better-sqlite3 for edge deployment compatibility"
"Using Bun for fast startup and native TypeScript support"
"Avoided React Query - project already uses SWR"
"Implemented refresh tokens for extended session support"
```

### `gotcha`
Pitfalls, edge cases, things that cause problems.

```
"Build fails silently if prebuild hasn't run"
"Can't import from @/lib directly in tests, must use relative paths"
"The linter enforces semicolons - ESLint rule semi: error"
"WebSocket connection drops if idle > 30s, needs keepalive"
```

### `pattern`
Reusable patterns and conventions in the codebase.

```
"Error handling uses Result<T, E> pattern throughout"
"API responses wrapped in { data, error, status } envelope"
"Components follow Container/Presenter pattern"
"All database queries go through repository classes"
```

## Extraction Points

### 1. UserPromptSubmit

When user sends a new message, extract insights from the **previous segment**.

**Why this boundary:**
- User message = new direction or context
- Corrections are captured immediately
- Natural conversational boundary

**Special handling:**
- Scan for correction signals (regex patterns)
- If correction detected, prioritize preference extraction
- Record new user intent for next segment

### 2. PreCompact

Extract **before** auto-compaction compresses context.

**Why this boundary:**
- Full detail available right before it's lost
- Prevents information loss in long sessions
- Natural "chapter break" in the work

**Behavior:**
- Extract accumulated segment
- Clear accumulator (context about to be compressed anyway)
- Start fresh accumulation

### 3. Stop

End of Claude's response. Final extraction for the session.

**Behavior:**
- Extract any remaining accumulated work
- Run deduplication pass on session memories
- Mark session as complete

## Accumulator Design

The accumulator collects lightweight summaries during a segment, NOT raw tool responses.

```typescript
type SegmentAccumulator = {
  sessionId: string;
  projectId: string;
  segmentId: string;           // UUID for this segment
  segmentStart: number;        // Timestamp

  // User context
  userPrompts: UserPrompt[];   // User messages in this segment

  // Work summary (paths and summaries only, not content)
  filesRead: Set<string>;
  filesModified: Set<string>;
  commandsRun: CommandSummary[];
  errorsEncountered: ErrorSummary[];
  searchesPerformed: SearchSummary[];

  // Claude's output
  lastAssistantMessage?: string;

  // Metrics
  toolCallCount: number;
};

type UserPrompt = {
  content: string;
  timestamp: number;
  signal: SignalClassification;  // From Haiku inference
};

type CommandSummary = {
  command: string;           // Truncated to 200 chars
  exitCode?: number;
  hasError: boolean;
};

type ErrorSummary = {
  source: string;            // Tool name or command
  message: string;           // Truncated to 500 chars
};

type SearchSummary = {
  tool: 'Grep' | 'Glob';
  pattern: string;
  resultCount: number;
};
```

**Size constraints:**
- `filesRead`, `filesModified`: Max 100 paths each
- `commandsRun`: Max 50 entries
- `errorsEncountered`: Max 20 entries
- `userPrompts`: No limit (natural boundary prevents growth)

## User Signal Detection

On UserPromptSubmit, use a quick Haiku inference to classify the user's message. Regex is brittle - "no problem, continue" would false-positive on a regex check for "no".

### Signal Classification Prompt

```
Classify this user message to Claude Code. Respond with JSON only.

Message: "{user_prompt}"

Categories:
- correction: User is correcting Claude's approach or output
- preference: User is stating a preference for how things should be done
- context: User is providing background information about the codebase/project
- task: User is giving a new task or continuing work
- question: User is asking a question
- feedback: User is giving positive/negative feedback without correction

```json
{
  "category": "correction|preference|context|task|question|feedback",
  "extractable": true|false,
  "summary": "One sentence summary if extractable, null otherwise"
}
```

Guidelines:
- "extractable" is true if this message contains information worth remembering
- corrections and preferences are almost always extractable
- context is often extractable
- tasks, questions, and feedback are rarely extractable
- summary should capture the preference/correction/context if present
```

### Signal Classification Response

```typescript
type SignalClassification = {
  category: 'correction' | 'preference' | 'context' | 'task' | 'question' | 'feedback';
  extractable: boolean;
  summary: string | null;
};
```

### Configuration

```json
{
  "signalDetection": {
    "model": "claude-haiku-4-20250514",
    "maxTokens": 150,
    "temperature": 0.0
  }
}
```

### Signal Detection Flow

```typescript
async function classifyUserSignal(prompt: string): Promise<SignalClassification> {
  // Quick Haiku inference
  const response = await agentQuery({
    model: config.signalDetection.model,
    maxTokens: config.signalDetection.maxTokens,
    temperature: config.signalDetection.temperature,
    prompt: buildSignalClassificationPrompt(prompt),
  });

  return parseSignalClassification(response);
}
```

### How Classification Affects Extraction

| Category | Extraction Behavior |
|----------|---------------------|
| `correction` | Priority extraction, bias toward `preference` type, include summary in prompt |
| `preference` | Priority extraction, bias toward `preference` type, include summary in prompt |
| `context` | Include summary in extraction prompt as codebase context |
| `task` | Standard extraction (no special handling) |
| `question` | Standard extraction (no special handling) |
| `feedback` | Standard extraction (no special handling) |

When `extractable: true`, the `summary` is passed to the main extraction prompt to help it identify what's worth remembering:

```
**User Signal Detected:**
Category: {category}
Summary: {summary}

This should likely become a memory. Extract it.
```

## Extraction Service

### Model Configuration

```typescript
type ExtractionConfig = {
  model: string;              // Default: "claude-sonnet-4-20250514"
  maxTokens: number;          // Default: 1024
  temperature: number;        // Default: 0.3 (low for consistency)
};
```

Settings in `~/.config/ccmemory/config.json`:
```json
{
  "extraction": {
    "model": "claude-sonnet-4-20250514",
    "maxTokens": 1024,
    "temperature": 0.3
  }
}
```

Uses Claude Code subscription via Agent SDK (no additional cost).

### Extraction Prompt

```
You are a memory extraction agent. Your job is to extract USEFUL insights from a Claude Code work segment that would help in FUTURE sessions.

## Work Segment

**User Prompts:**
{user_prompts}

**Files Read:** {files_read}
**Files Modified:** {files_modified}
**Commands Run:** {commands_run}
**Errors Encountered:** {errors_encountered}

**Claude's Response:**
{last_assistant_message}

## Instructions

Extract memories ONLY if they would be valuable in future sessions. Most segments produce 0-3 memories. Many produce none.

**Memory Types:**
- `preference`: User preference or correction (HIGHEST PRIORITY if user corrected something)
- `codebase`: How the codebase works (architecture, structure, conventions)
- `decision`: Architectural choice with rationale (the WHY)
- `gotcha`: Pitfall or gotcha that caused problems
- `pattern`: Reusable pattern or convention

**What to extract:**
- User corrections or stated preferences
- Discoveries about codebase architecture
- Decisions made and WHY
- Things that caused errors or confusion
- Patterns observed in the code

**What to SKIP:**
- Routine operations (file reads, installs, status checks)
- Obvious things (standard library usage, common patterns)
- Temporary or one-off changes
- Information only relevant to current task

## Output Format

Return JSON array (empty array if nothing worth remembering):

```json
[
  {
    "type": "preference|codebase|decision|gotcha|pattern",
    "content": "Concise, standalone statement of the insight",
    "context": "Brief context of how this was discovered (1 sentence)",
    "confidence": 0.0-1.0,
    "relatedFiles": ["path/to/file.ts"]
  }
]
```

**Guidelines:**
- `content` should be self-contained and useful without context
- `confidence` reflects certainty (user stated = 1.0, inferred = 0.5-0.8)
- `relatedFiles` only if directly relevant (not every file touched)
- Maximum 5 memories per extraction (prefer fewer, higher quality)
```

### Extraction Response Schema

```typescript
type ExtractionResponse = ExtractedMemory[];

type ExtractedMemory = {
  type: 'preference' | 'codebase' | 'decision' | 'gotcha' | 'pattern';
  content: string;
  context: string;
  confidence: number;
  relatedFiles: string[];
};
```

### Extraction Flow

```
1. Build extraction prompt from accumulator
2. Call extraction model via Agent SDK
3. Parse JSON response
4. Validate each memory (type, content length, confidence range)
5. Check for duplicates within session
6. Check for superseding against existing memories
7. Store valid memories
8. Clear accumulator
```

## Superseding Detection

When a new memory might contradict an existing one, use inference to detect.

### When to Check

Check superseding when:
- New `preference` memory extracted
- New memory has high confidence (>= 0.8)
- New memory's content has semantic overlap with existing memories

### Superseding Prompt

```
You are checking if a new memory supersedes (replaces/updates) an existing memory.

**Existing Memory:**
Type: {existing.type}
Content: {existing.content}
Created: {existing.createdAt}

**New Memory:**
Type: {new.type}
Content: {new.content}

**Question:** Does the new memory supersede (replace, update, or contradict) the existing memory?

Respond with JSON:
```json
{
  "supersedes": true|false,
  "reason": "Brief explanation"
}
```

**Guidelines:**
- Supersedes if: new memory contradicts, updates, or refines the existing one
- Does NOT supersede if: memories are about different topics, or complementary
- When in doubt, return false (keep both memories)
```

### Superseding Flow

```
1. For each new memory with confidence >= 0.8
2. Search existing memories by type and semantic similarity
3. For each candidate (similarity > 0.7)
4. Run superseding check
5. If supersedes: mark old memory as superseded, link to new
6. Store new memory with supersedes_id if applicable
```

## Data Model

### memories table

```sql
CREATE TABLE memories (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  session_id TEXT,              -- NULL if promoted to project-level
  segment_id TEXT,              -- Which segment this came from

  type TEXT NOT NULL,           -- preference|codebase|decision|gotcha|pattern
  content TEXT NOT NULL,
  context TEXT,
  confidence REAL DEFAULT 0.5,

  related_files TEXT,           -- JSON array of file paths

  -- Superseding
  superseded_by_id TEXT,        -- If superseded, points to newer memory
  supersedes_id TEXT,           -- If this supersedes another, points to it

  -- Embeddings
  embedding BLOB,               -- Vector embedding for semantic search

  -- Metadata
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now')),

  -- Soft delete
  deleted_at TEXT,

  FOREIGN KEY (project_id) REFERENCES projects(id),
  FOREIGN KEY (superseded_by_id) REFERENCES memories(id),
  FOREIGN KEY (supersedes_id) REFERENCES memories(id)
);

CREATE INDEX idx_memories_project_type ON memories(project_id, type);
CREATE INDEX idx_memories_superseded ON memories(superseded_by_id);
```

### extraction_segments table

Track extraction history for debugging and deduplication.

```sql
CREATE TABLE extraction_segments (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  project_id TEXT NOT NULL,

  trigger TEXT NOT NULL,        -- user_prompt|pre_compact|stop

  -- Input summary
  user_prompts TEXT,            -- JSON array
  files_read TEXT,              -- JSON array
  files_modified TEXT,          -- JSON array
  tool_call_count INTEGER,

  -- Output
  memories_extracted INTEGER DEFAULT 0,
  extraction_tokens INTEGER,    -- Tokens used for extraction

  -- Timing
  segment_start TEXT,
  segment_end TEXT,
  extraction_duration_ms INTEGER,

  created_at TEXT DEFAULT (datetime('now'))
);
```

## Hook Handlers

### UserPromptSubmit Handler

```typescript
async function handleUserPromptSubmit(input: UserPromptSubmitInput): Promise<HookOutput> {
  const { session_id, cwd, prompt } = input;

  // 1. Classify user signal with quick Haiku inference
  const signal = await classifyUserSignal(prompt);

  // 2. Check if we have accumulated work to extract
  const accumulator = await getAccumulator(session_id);

  if (accumulator && accumulator.toolCallCount > 0) {
    // Extract from previous segment, including signal context
    await extractSegment(accumulator, 'user_prompt', signal);
  }

  // 3. Start new segment
  await startNewSegment(session_id, cwd, {
    userPrompt: prompt,
    signal,
  });

  // 4. Return (don't block, don't inject context)
  return { continue: true };
}
```

### PostToolUse Handler

```typescript
async function handlePostToolUse(input: PostToolUseInput): Promise<HookOutput> {
  const { session_id, tool_name, tool_input, tool_response } = input;

  // Lightweight accumulation - summaries only, not full content
  const accumulator = await getOrCreateAccumulator(session_id);

  accumulator.toolCallCount++;

  switch (tool_name) {
    case 'Read':
      accumulator.filesRead.add(tool_input.file_path);
      break;

    case 'Write':
    case 'Edit':
      accumulator.filesModified.add(tool_input.file_path);
      break;

    case 'Bash':
      accumulator.commandsRun.push({
        command: tool_input.command?.slice(0, 200) ?? '',
        exitCode: tool_response?.exitCode,
        hasError: tool_response?.exitCode !== 0,
      });

      if (tool_response?.stderr) {
        accumulator.errorsEncountered.push({
          source: 'Bash',
          message: tool_response.stderr.slice(0, 500),
        });
      }
      break;

    case 'Grep':
    case 'Glob':
      accumulator.searchesPerformed.push({
        tool: tool_name,
        pattern: tool_input.pattern ?? '',
        resultCount: Array.isArray(tool_response) ? tool_response.length : 0,
      });
      break;
  }

  await saveAccumulator(accumulator);

  return { continue: true };
}
```

### PreCompact Handler

```typescript
async function handlePreCompact(input: PreCompactInput): Promise<HookOutput> {
  const { session_id, trigger } = input;

  const accumulator = await getAccumulator(session_id);

  if (accumulator && accumulator.toolCallCount > 0) {
    // Extract before context is lost
    await extractSegment(accumulator, 'pre_compact');

    // Clear accumulator - context about to be compressed
    await clearAccumulator(session_id);
  }

  return { continue: true };
}
```

### Stop Handler

```typescript
async function handleStop(input: StopInput): Promise<HookOutput> {
  const { session_id, transcript_path } = input;

  const accumulator = await getAccumulator(session_id);

  if (accumulator && accumulator.toolCallCount > 0) {
    // Get Claude's last response from transcript
    const lastMessage = await getLastAssistantMessage(transcript_path);
    accumulator.lastAssistantMessage = lastMessage;

    // Final extraction
    await extractSegment(accumulator, 'stop');
  }

  // Run session deduplication
  await deduplicateSessionMemories(session_id);

  // Clear accumulator
  await clearAccumulator(session_id);

  return { continue: true };
}
```

### SessionStart Handler

```typescript
async function handleSessionStart(input: SessionStartInput): Promise<HookOutput> {
  const { session_id, cwd, source } = input;

  // Initialize session tracking
  await initializeSession(session_id, cwd);

  // NO context injection - memories are searched on-demand via MCP tools

  return { continue: true };
}
```

## MCP Tools

Memories are accessed via MCP tools, not auto-injected.

### memory_search

Search memories by query, type, or project.

```typescript
{
  name: 'memory_search',
  description: 'Search memories by semantic similarity and keywords',
  parameters: {
    query: string,              // Search query
    type?: MemoryType,          // Filter by type
    limit?: number,             // Max results (default 10)
    includeSuperseded?: boolean // Include superseded memories (default false)
  }
}
```

### memory_add

Manually add a memory (for explicit user notes).

```typescript
{
  name: 'memory_add',
  description: 'Manually add a memory',
  parameters: {
    content: string,
    type: MemoryType,
    context?: string,
    confidence?: number         // Default 1.0 for manual
  }
}
```

### memory_supersede

Manually mark a memory as superseded.

```typescript
{
  name: 'memory_supersede',
  description: 'Mark one memory as superseding another',
  parameters: {
    oldMemoryId: string,
    newMemoryId: string
  }
}
```

## Hooks Configuration

```json
{
  "description": "CCMemory: Conceptual memory extraction",
  "hooks": {
    "UserPromptSubmit": [
      {
        "hooks": [{
          "type": "command",
          "command": "${CLAUDE_PLUGIN_ROOT}/bin/ccmemory hook user-prompt",
          "timeout": 30
        }]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "*",
        "hooks": [{
          "type": "command",
          "command": "${CLAUDE_PLUGIN_ROOT}/bin/ccmemory hook post-tool",
          "timeout": 5
        }]
      }
    ],
    "PreCompact": [
      {
        "matcher": "*",
        "hooks": [{
          "type": "command",
          "command": "${CLAUDE_PLUGIN_ROOT}/bin/ccmemory hook pre-compact",
          "timeout": 60
        }]
      }
    ],
    "Stop": [
      {
        "hooks": [{
          "type": "command",
          "command": "${CLAUDE_PLUGIN_ROOT}/bin/ccmemory hook stop",
          "timeout": 60
        }]
      }
    ],
    "SessionStart": [
      {
        "hooks": [{
          "type": "command",
          "command": "${CLAUDE_PLUGIN_ROOT}/bin/ccmemory hook session-start",
          "timeout": 10
        }]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [{
          "type": "command",
          "command": "${CLAUDE_PLUGIN_ROOT}/bin/ccmemory hook session-end",
          "timeout": 10
        }]
      }
    ]
  }
}
```

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        Claude Code Session                               │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  SessionStart ──────────────────────────────────────▶ Initialize       │
│                                                       (no injection)    │
│                                                                         │
│  UserPromptSubmit ─────┬──▶ Extract previous segment                   │
│         │              │    Detect corrections                          │
│         │              │    Start new segment                           │
│         ▼              │                                                │
│  ┌─────────────────┐   │                                                │
│  │   Accumulator   │   │                                                │
│  │  ┌───────────┐  │   │                                                │
│  │  │filesRead  │  │   │                                                │
│  │  │filesModif │  │   │                                                │
│  │  │commands   │  │   │                                                │
│  │  │errors     │  │   │                                                │
│  │  │prompts    │  │   │                                                │
│  │  └───────────┘  │   │                                                │
│  └─────────────────┘   │                                                │
│         ▲              │                                                │
│         │              │                                                │
│  PostToolUse ──────────┼──▶ Lightweight accumulation                   │
│  PostToolUse ──────────┤    (summaries only)                           │
│  PostToolUse ──────────┤                                                │
│                        │                                                │
│  PreCompact ───────────┼──▶ Extract before context lost                │
│                        │    Clear accumulator                           │
│                        │                                                │
│  Stop ─────────────────┴──▶ Final extraction                           │
│                             Deduplicate session                         │
│                             Clear accumulator                           │
│                                                                         │
│  SessionEnd ───────────────▶ Cleanup                                   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        Extraction Service                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌──────────────┐    ┌───────────────┐    ┌─────────────────────────┐  │
│  │  Accumulator │───▶│   Extraction  │───▶│   Superseding Check     │  │
│  │    Data      │    │    Prompt     │    │   (if high confidence)  │  │
│  └──────────────┘    └───────────────┘    └─────────────────────────┘  │
│                              │                         │                │
│                              ▼                         ▼                │
│                      ┌───────────────┐         ┌─────────────┐         │
│                      │  Agent SDK    │         │  Mark Old   │         │
│                      │  (Claude)     │         │  Superseded │         │
│                      └───────────────┘         └─────────────┘         │
│                              │                         │                │
│                              ▼                         ▼                │
│                      ┌───────────────┐         ┌─────────────┐         │
│                      │    Parse &    │         │   Store     │         │
│                      │   Validate    │────────▶│   Memory    │         │
│                      └───────────────┘         └─────────────┘         │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          Memory Store                                    │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                         memories table                           │   │
│  │  ┌──────┬──────────┬─────────┬────────────┬──────────────────┐  │   │
│  │  │  id  │   type   │ content │ confidence │ superseded_by_id │  │   │
│  │  ├──────┼──────────┼─────────┼────────────┼──────────────────┤  │   │
│  │  │ m001 │preference│ Use...  │    1.0     │       NULL       │  │   │
│  │  │ m002 │ codebase │ Auth... │    0.8     │       NULL       │  │   │
│  │  │ m003 │preference│ Old...  │    0.9     │       m001       │  │   │
│  │  └──────┴──────────┴─────────┴────────────┴──────────────────┘  │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    Vector Index (Embeddings)                     │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           MCP Tools                                      │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  memory_search ──────▶ Search by query, type, project                  │
│  memory_add ─────────▶ Manual memory creation                          │
│  memory_supersede ───▶ Manual superseding                              │
│  memory_timeline ────▶ Chronological view                              │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Example Scenarios

### Scenario 1: User Correction

```
User: "Add authentication"
[Claude implements with localStorage]
User: "No, use httpOnly cookies instead of localStorage"

Extraction at UserPromptSubmit:
[
  {
    "type": "preference",
    "content": "Store authentication tokens in httpOnly cookies, not localStorage",
    "context": "User corrected implementation approach",
    "confidence": 1.0,
    "relatedFiles": ["src/auth/token.ts"]
  }
]
```

### Scenario 2: Codebase Discovery

```
User: "Fix the login bug"
[Claude reads auth/, discovers JWT setup, fixes bug]

Extraction at Stop:
[
  {
    "type": "codebase",
    "content": "Authentication uses JWT with refresh tokens, handled in src/auth/",
    "context": "Discovered while investigating login bug",
    "confidence": 0.85,
    "relatedFiles": ["src/auth/jwt.ts", "src/auth/refresh.ts"]
  },
  {
    "type": "gotcha",
    "content": "JWT refresh endpoint requires both access and refresh tokens in request",
    "context": "Root cause of login bug",
    "confidence": 0.9,
    "relatedFiles": ["src/auth/refresh.ts"]
  }
]
```

### Scenario 3: Long Session with Compaction

```
T+0:00  User: "Refactor the API layer"
T+0:30  [100 tool calls]
T+0:30  PreCompact (auto)
        → Extract: codebase memory about API structure

T+0:45  User: "Also add rate limiting"
        → Extract: nothing notable from refactor segment

T+1:15  PreCompact (auto)
        → Extract: decision about rate limiting approach

T+1:30  User: "Use Redis, not in-memory"
        → Extract: preference for Redis over in-memory

T+2:00  Stop
        → Extract: pattern about rate limiting implementation
        → Deduplicate: combine related rate limiting memories
```

### Scenario 4: Nothing Worth Remembering

```
User: "Run the tests"
[Claude runs tests, all pass]

Extraction at Stop:
[]  // Empty - routine operation, nothing to remember
```

## Configuration

### Settings File

`~/.config/ccmemory/config.json`:

```json
{
  "extraction": {
    "model": "claude-sonnet-4-20250514",
    "maxTokens": 1024,
    "temperature": 0.3,
    "minToolCallsToExtract": 3
  },
  "signalDetection": {
    "model": "claude-haiku-4-20250514",
    "maxTokens": 150,
    "temperature": 0.0
  },
  "superseding": {
    "model": "claude-haiku-4-20250514",
    "similarityThreshold": 0.7,
    "confidenceThreshold": 0.8
  },
  "accumulator": {
    "maxFilesTracked": 100,
    "maxCommandsTracked": 50,
    "maxErrorsTracked": 20
  },
  "embedding": {
    "provider": "ollama",
    "model": "nomic-embed-text",
    "fallback": {
      "provider": "openrouter",
      "model": "text-embedding-3-small"
    }
  }
}
```

### Environment Variables

```bash
CCMEMORY_DATA_DIR      # Data directory (default: XDG data dir)
CCMEMORY_CONFIG_DIR    # Config directory (default: XDG config dir)
LOG_LEVEL              # debug|info|warn|error (default: info)
OPENROUTER_API_KEY     # For embedding fallback
```

## Migration from Current Implementation

The current CCMemory implementation stores per-tool observations. Migration path:

1. **Schema migration**: Add new columns, keep old data
2. **Parallel operation**: New extraction runs alongside old capture
3. **Old data**: Mark as `legacy` type, keep for reference
4. **Deprecation**: Remove old capture hooks after validation
5. **Cleanup**: Optional migration of valuable legacy memories

## Success Metrics

- **Memory quality**: Manual review of extracted memories for usefulness
- **Memory volume**: Should be 1-5 memories per hour of work (not 50+)
- **Superseding accuracy**: Track false positives/negatives in superseding
- **User corrections captured**: % of corrections that become memories
- **Extraction latency**: Should not noticeably slow Claude Code

## Future Considerations

1. **Cross-project memories**: Some preferences apply globally
2. **Memory decay**: Old, unused memories could be archived
3. **Memory validation**: Let user confirm/reject extracted memories
4. **Team sharing**: Share codebase memories across team members
5. **Memory triggers**: Surface relevant memories during conversation (opt-in)
