import { Loader2, Search as SearchIcon } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import type { Memory, MemorySector, MemoryType } from '../../services/memory/types.js';
import type { SearchResult } from '../../services/search/hybrid.js';
import { MemoryCard } from './MemoryCard.js';
import { Button } from './ui/button.js';
import { Checkbox } from './ui/checkbox.js';
import { Input } from './ui/input.js';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select.js';

const memoryTypeLabels: Record<MemoryType, string> = {
  preference: 'Preference',
  codebase: 'Codebase',
  decision: 'Decision',
  gotcha: 'Gotcha',
  pattern: 'Pattern',
  turn_summary: 'Turn Summary',
  task_completion: 'Task Completion',
};

type SearchProps = {
  initialResults: SearchResult[];
  onSelectMemory: (memory: Memory) => void;
  wsConnected: boolean;
};

export function Search({ initialResults, onSelectMemory, wsConnected }: SearchProps): React.JSX.Element {
  const [query, setQuery] = useState('');
  const [sector, setSector] = useState<MemorySector | 'all'>('all');
  const [memoryType, setMemoryType] = useState<MemoryType | 'all'>('all');
  const [includeSuperseded, setIncludeSuperseded] = useState(false);
  const [results, setResults] = useState(initialResults);
  const [loading, setLoading] = useState(false);
  const [projectId] = useState<string | null>(() => {
    if (typeof window === 'undefined') return null;
    return new URLSearchParams(window.location.search).get('project');
  });
  const [sessionId] = useState<string | null>(() => {
    if (typeof window === 'undefined') return null;
    return new URLSearchParams(window.location.search).get('session');
  });

  useEffect(() => {
    setResults(initialResults);
  }, [initialResults]);

  const handleSearch = useCallback(
    async (e?: React.FormEvent) => {
      e?.preventDefault();
      if (!query.trim() && memoryType === 'all') return;

      setLoading(true);
      try {
        const params = new URLSearchParams();
        if (query.trim()) params.set('q', query);
        if (sector !== 'all') params.set('sector', sector);
        if (memoryType !== 'all') params.set('memory_type', memoryType);
        if (includeSuperseded) params.set('include_superseded', 'true');
        if (projectId) params.set('project', projectId);
        if (sessionId) params.set('session', sessionId);

        const res = await fetch(`/api/search?${params}`);
        const data = (await res.json()) as { results: SearchResult[] };
        setResults(data.results);
        if (typeof window !== 'undefined') {
          window.history.pushState({}, '', `/search?${params}`);
        }
      } finally {
        setLoading(false);
      }
    },
    [query, sector, memoryType, includeSuperseded, projectId, sessionId],
  );

  return (
    <div className="space-y-6">
      <form onSubmit={handleSearch} className="flex flex-col gap-4 sm:flex-row sm:items-center">
        <div className="relative flex-1">
          <SearchIcon className="absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            type="text"
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="Search memories..."
            className="pl-10"
            autoFocus
          />
        </div>

        <Select value={sector} onValueChange={v => setSector(v as MemorySector | 'all')}>
          <SelectTrigger className="w-[140px]">
            <SelectValue placeholder="All Sectors" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All Sectors</SelectItem>
            <SelectItem value="episodic">Episodic</SelectItem>
            <SelectItem value="semantic">Semantic</SelectItem>
            <SelectItem value="procedural">Procedural</SelectItem>
            <SelectItem value="emotional">Emotional</SelectItem>
            <SelectItem value="reflective">Reflective</SelectItem>
          </SelectContent>
        </Select>

        <Select value={memoryType} onValueChange={v => setMemoryType(v as MemoryType | 'all')}>
          <SelectTrigger className="w-[160px]">
            <SelectValue placeholder="All Types" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All Types</SelectItem>
            {Object.entries(memoryTypeLabels).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <div className="flex items-center space-x-2">
          <Checkbox
            id="superseded"
            checked={includeSuperseded}
            onCheckedChange={checked => setIncludeSuperseded(checked === true)}
          />
          <label htmlFor="superseded" className="cursor-pointer text-sm text-muted-foreground">
            Include superseded
          </label>
        </div>

        <Button type="submit" disabled={loading}>
          {loading ? (
            <>
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              Searching...
            </>
          ) : (
            'Search'
          )}
        </Button>
      </form>

      <div className="flex items-center gap-2 text-sm">
        {wsConnected ? (
          <span className="flex items-center gap-1.5 text-green-600 dark:text-green-400">
            <span className="h-2 w-2 animate-pulse rounded-full bg-green-500" />
            Live updates enabled
          </span>
        ) : (
          <span className="flex items-center gap-1.5 text-muted-foreground">
            <span className="h-2 w-2 rounded-full bg-muted" />
            Connecting...
          </span>
        )}
      </div>

      <div className="space-y-4">
        {results.length === 0 ? (
          <p className="py-12 text-center text-muted-foreground">
            {query
              ? 'No memories found.'
              : projectId
                ? 'No memories found for this project.'
                : sessionId
                  ? 'No memories found for this session.'
                  : 'Enter a search query to find memories.'}
          </p>
        ) : (
          <>
            {!query && projectId && (
              <p className="text-sm text-muted-foreground">Showing {results.length} recent memories for this project</p>
            )}
            {!query && sessionId && (
              <p className="text-sm text-muted-foreground">Showing {results.length} recent memories for this session</p>
            )}
            {results.map(r => (
              <MemoryCard key={r.memory.id} result={r} onClick={() => onSelectMemory(r.memory)} />
            ))}
          </>
        )}
      </div>
    </div>
  );
}
