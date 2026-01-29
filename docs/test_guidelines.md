# Testing Strategies

The following strategies will actually test if the code works, not what functions do:

## 1. End-to-End Flow Tests

Replace unit tests with integration tests that test complete workflows:

```rust
#[test]
async fn test_index_and_search_flow() {
    // Setup: Create a temporary project with real files
    let dir = tempdir();
    write_file(&dir, "src/lib.rs", "pub fn hello() {}");

    // Act: Index the project, then search
    let daemon = start_daemon(&dir).await;
    daemon.index_project().await;
    let results = daemon.search("hello").await;

    // Assert: Found the function
    assert!(results.iter().any(|r| r.symbol == "hello"));
}
```

## 2. Scenario-Based Tests

Test user scenarios, not implementation:

```rust
#[test]
async fn test_memory_lifecycle() {
    // User creates a memory
    let memory_id = daemon.add_memory("User prefers dark mode").await;

    // Memory is found in search
    let found = daemon.search_memories("dark mode").await;
    assert!(found.contains(&memory_id));

    // Memory decays over time
    advance_time(30.days());
    daemon.apply_decay().await;

    // Low salience memory is still found but ranked lower
    let results = daemon.search_memories("dark mode").await;
    assert!(results[0].salience < 0.5);
}
```

## 3. Contract Tests

Test API contracts, not internals:

```rust
#[test]
fn test_mcp_explore_response_contract() {
    // The response must be valid JSON that matches the MCP spec
    let response = format_explore_response(&results);

    // Parse and validate against schema
    let parsed: serde_json::Value = serde_json::from_str(&response)?;
    assert!(parsed["results"].is_array());
    for result in parsed["results"].as_array().unwrap() {
        assert!(result["file"].is_string() || result["file"].is_null());
        assert!(result["preview"].is_string());
    }
}
```

## 4. Property-Based Tests

Test invariants, not specific values:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn chunking_preserves_content(content: String) {
        let chunks = chunk_text(&content, 500, 50);

        // Property: All content is represented in chunks
        let reconstructed: String = chunks.iter()
            .map(|c| &c.text)
            .collect();
        assert!(content.chars().all(|c| reconstructed.contains(c)));
    }

    #[test]
    fn ranking_is_stable(memories: Vec<Memory>) {
        let ranked1 = rank_memories(&memories, &weights);
        let ranked2 = rank_memories(&memories, &weights);

        // Property: Same input produces same ranking
        assert_eq!(ranked1, ranked2);
    }
}
```

## 5. Regression Tests

Test actual bugs that have occurred:

```rust
#[test]
fn regression_issue_123_unicode_truncation() {
    // Bug: Truncation at byte boundary broke UTF-8
    let text = "Hello 你好 World";
    let truncated = truncate_text(text, 10);

    // Must be valid UTF-8
    assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
}
```

## What NOT to Test

1. **Default values** - If they're wrong, you'll notice immediately
2. **Type conversions** - The compiler checks these
3. **Display/FromStr** - These are formatting, not logic
4. **Struct field assignment** - This is what Rust's type system is for
5. **Simple math** - `50/100 = 0.5` doesn't need a test
6. **Standard library behavior** - `HashMap::insert` works
7. **Trivial functions** - simple functions with no branching or complex logic

## Test Organization

For single-domain tests, use normal rust `mod tests` in the relevant module. For integration and e2e tests (the most important kind that should always be the main focus of testing), use a `__tests__` directory in the relevant top-level module folder. For example:

```
crates/backend/src/actor
├── mod.rs
├── __tests__
│   ├── mod.rs
│   └── watcher.rs
└── watcher.rs
```

Where `actor/mod.rs` contains:

```rust
mod watcher;

#[cfg(test)]
mod __tests__;
```

This way, the tests can access watcher's internals without needing to be public, while still being clearly separated from production code and not bloating the main module file. Benchmarks should go in the `__tests__` folder as well, but have their name end in `_bench.rs`. Benchmarks are done with `criterion`.
