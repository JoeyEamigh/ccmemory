//! Lifecycle E2E tests for auto-reindex and watcher behavior.
//!
//! Tests the startup scan feature that detects file changes when daemon restarts,
//! and watcher live change detection.

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use crate::actor::__tests__::helpers::{
    ActorTestContext, search_code, start_watcher, stop_watcher, trigger_index, wait_for, wait_for_scan_complete,
  };

  // ==========================================================================
  // Auto-Reindex on Startup Tests
  // ==========================================================================

  /// Test: Index -> stop -> modify file -> restart -> auto-detect and reindex.
  #[tokio::test]
  async fn test_startup_reindex_detects_modified_file() {
    let ctx = ActorTestContext::new().await;

    // Setup: create and manually index
    ctx.write_source_file("src/lib.rs", "pub fn original() {}").await;
    let (handle1, cancel1) = ctx.spawn_project_actor().await.expect("spawn actor");
    let index_result = trigger_index(&handle1).await.expect("index should succeed");
    assert!(index_result.files_indexed > 0, "Should index at least one file");

    // Verify original content is searchable
    let search = search_code(&handle1, "original").await.expect("search should work");
    assert!(!search.chunks.is_empty(), "Should find original content");

    // Stop the actor
    cancel1.cancel();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Modify file while actor is down
    ctx.write_source_file("src/lib.rs", "pub fn modified() {}").await;

    // Restart - should AUTO-DETECT and queue for reindex
    let (handle2, cancel2) = ctx.spawn_project_actor().await.expect("spawn actor");

    // Start watcher which triggers startup scan for previously indexed projects
    start_watcher(&handle2).await.expect("start watcher");

    // Wait for scan to complete
    assert!(
      wait_for_scan_complete(&handle2, Duration::from_secs(10)).await,
      "Startup scan should complete"
    );

    // Wait a bit more for reindexing to complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Assert: new content found
    let search = search_code(&handle2, "modified").await.expect("search");
    assert!(!search.chunks.is_empty(), "Should find modified content");

    let search_old = search_code(&handle2, "original").await.expect("search");
    assert!(
      search_old.chunks.is_empty() || !search_old.chunks.iter().any(|c| c.content.contains("original")),
      "Old content should be gone or minimal"
    );

    cancel2.cancel();
    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  /// Test: Index -> stop -> add file -> restart -> new file auto-indexed.
  #[tokio::test]
  async fn test_startup_reindex_detects_added_file() {
    let ctx = ActorTestContext::new().await;

    // Setup: create and index initial file
    ctx.write_source_file("src/lib.rs", "pub fn initial() {}").await;
    let (handle1, cancel1) = ctx.spawn_project_actor().await.expect("spawn actor");
    trigger_index(&handle1).await.expect("index should succeed");

    // Stop the actor
    cancel1.cancel();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Add a new file while actor is down
    ctx
      .write_source_file("src/new_file.rs", "pub fn brand_new_function() {}")
      .await;

    // Restart
    let (handle2, cancel2) = ctx.spawn_project_actor().await.expect("spawn actor");
    start_watcher(&handle2).await.expect("start watcher");
    wait_for_scan_complete(&handle2, Duration::from_secs(10)).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Assert: new file should be indexed
    let search = search_code(&handle2, "brand_new_function").await.expect("search");
    assert!(!search.chunks.is_empty(), "Should find new file content");

    cancel2.cancel();
  }

  /// Test: Index -> stop -> delete file -> restart -> old chunks removed.
  #[tokio::test]
  async fn test_startup_reindex_detects_deleted_file() {
    let ctx = ActorTestContext::new().await;

    // Setup: create and index files
    ctx.write_source_file("src/lib.rs", "pub mod keep;").await;
    ctx.write_source_file("src/keep.rs", "pub fn keep_me() {}").await;
    ctx.write_source_file("src/delete.rs", "pub fn delete_me() {}").await;

    let (handle1, cancel1) = ctx.spawn_project_actor().await.expect("spawn actor");
    trigger_index(&handle1).await.expect("index should succeed");

    // Verify both are searchable
    let search = search_code(&handle1, "delete_me").await.expect("search");
    assert!(!search.chunks.is_empty(), "Should find delete_me before deletion");

    // Stop the actor
    cancel1.cancel();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Delete a file while actor is down
    ctx.delete_source_file("src/delete.rs").await;

    // Restart
    let (handle2, cancel2) = ctx.spawn_project_actor().await.expect("spawn actor");
    start_watcher(&handle2).await.expect("start watcher");
    wait_for_scan_complete(&handle2, Duration::from_secs(10)).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Assert: deleted file should be gone from index
    let search = search_code(&handle2, "delete_me").await.expect("search");
    assert!(
      search.chunks.is_empty() || !search.chunks.iter().any(|c| c.content.contains("delete_me")),
      "Deleted file content should be removed from index"
    );

    // The kept file should still be there
    let search = search_code(&handle2, "keep_me").await.expect("search");
    assert!(!search.chunks.is_empty(), "Kept file should still be indexed");

    cancel2.cancel();
  }

  /// Test: Index -> stop -> rename file -> restart -> path updated, embeddings preserved.
  #[tokio::test]
  async fn test_startup_reindex_detects_moved_file() {
    let ctx = ActorTestContext::new().await;

    // Setup: create and index file
    ctx.write_source_file("src/old_name.rs", "pub fn move_me() {}").await;

    let (handle1, cancel1) = ctx.spawn_project_actor().await.expect("spawn actor");
    trigger_index(&handle1).await.expect("index should succeed");

    // Stop the actor
    cancel1.cancel();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Rename file while actor is down
    ctx.rename_source_file("src/old_name.rs", "src/new_name.rs").await;

    // Restart
    let (handle2, cancel2) = ctx.spawn_project_actor().await.expect("spawn actor");
    start_watcher(&handle2).await.expect("start watcher");
    wait_for_scan_complete(&handle2, Duration::from_secs(10)).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Assert: content should be searchable
    let search = search_code(&handle2, "move_me").await.expect("search");
    assert!(!search.chunks.is_empty(), "Should find renamed file content");

    // And the path should be the new one
    if !search.chunks.is_empty() {
      let has_new_path = search.chunks.iter().any(|c| c.file_path.contains("new_name"));
      let has_old_path = search.chunks.iter().any(|c| c.file_path.contains("old_name"));
      assert!(has_new_path, "Should have new file path");
      assert!(!has_old_path, "Should not have old file path");
    }

    cancel2.cancel();
  }

  /// Test: Never indexed -> spawn actor -> NO automatic scan happens.
  #[tokio::test]
  async fn test_no_auto_scan_if_never_indexed() {
    let ctx = ActorTestContext::new().await;

    // Create a file but don't index
    ctx
      .write_source_file("src/lib.rs", "pub fn should_not_auto_index() {}")
      .await;

    // Spawn actor (without calling trigger_index)
    let (handle, cancel) = ctx.spawn_project_actor().await.expect("spawn actor");

    // Start watcher - should NOT auto-scan since never indexed
    start_watcher(&handle).await.expect("start watcher");

    // Give it some time to potentially scan (it shouldn't)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Search should find nothing because auto-scan didn't happen
    let search = search_code(&handle, "should_not_auto_index").await.expect("search");
    assert!(
      search.chunks.is_empty(),
      "Should not auto-index without explicit index call"
    );

    cancel.cancel();
  }

  // ==========================================================================
  // Watcher Live Changes Tests
  // ==========================================================================

  /// Test: Watcher running -> create file -> becomes searchable.
  #[tokio::test]
  async fn test_watcher_indexes_new_file() {
    let ctx = ActorTestContext::new().await;

    // Create initial file and index
    ctx.write_source_file("src/lib.rs", "pub mod initial;").await;
    let (handle, cancel) = ctx.spawn_project_actor().await.expect("spawn actor");
    trigger_index(&handle).await.expect("index should succeed");

    // Start watcher
    start_watcher(&handle).await.expect("start watcher");
    wait_for_scan_complete(&handle, Duration::from_secs(5)).await;

    // Create a new file while watcher is running
    ctx
      .write_source_file("src/live_new.rs", "pub fn live_created() {}")
      .await;

    // Wait for watcher to pick it up (debounce + indexing)
    let found = wait_for(Duration::from_secs(10), || async {
      search_code(&handle, "live_created")
        .await
        .map(|r| !r.chunks.is_empty())
        .unwrap_or(false)
    })
    .await;

    assert!(found, "Watcher should index new file");

    cancel.cancel();
  }

  /// Test: Watcher -> modify file -> new content found, old gone.
  #[tokio::test]
  async fn test_watcher_updates_modified_file() {
    let ctx = ActorTestContext::new().await;

    // Create and index file
    ctx.write_source_file("src/lib.rs", "pub fn before_modify() {}").await;
    let (handle, cancel) = ctx.spawn_project_actor().await.expect("spawn actor");
    trigger_index(&handle).await.expect("index should succeed");

    // Verify initial content
    let search = search_code(&handle, "before_modify").await.expect("search");
    assert!(!search.chunks.is_empty(), "Should find initial content");

    // Start watcher
    start_watcher(&handle).await.expect("start watcher");
    wait_for_scan_complete(&handle, Duration::from_secs(5)).await;

    // Modify the file
    ctx.write_source_file("src/lib.rs", "pub fn after_modify() {}").await;

    // Wait for watcher to update AND old content to be replaced
    let updated = wait_for(Duration::from_secs(15), || async {
      let new_found = search_code(&handle, "after_modify")
        .await
        .map(|r| !r.chunks.is_empty())
        .unwrap_or(false);

      let old_gone = search_code(&handle, "before_modify")
        .await
        .map(|r| r.chunks.is_empty() || !r.chunks.iter().any(|c| c.content.contains("before_modify")))
        .unwrap_or(false);

      new_found && old_gone
    })
    .await;

    assert!(updated, "Watcher should replace old content with new content");

    cancel.cancel();
  }

  /// Test: Watcher -> delete file -> removed from index.
  #[tokio::test]
  async fn test_watcher_removes_deleted_file() {
    let ctx = ActorTestContext::new().await;

    // Create and index files
    ctx.write_source_file("src/lib.rs", "pub mod keep;").await;
    ctx.write_source_file("src/keep.rs", "pub fn keep_function() {}").await;
    ctx
      .write_source_file("src/delete.rs", "pub fn delete_function() {}")
      .await;

    let (handle, cancel) = ctx.spawn_project_actor().await.expect("spawn actor");
    trigger_index(&handle).await.expect("index should succeed");

    // Start watcher
    start_watcher(&handle).await.expect("start watcher");
    wait_for_scan_complete(&handle, Duration::from_secs(5)).await;

    // Delete file
    ctx.delete_source_file("src/delete.rs").await;

    // Wait for watcher to process deletion
    let removed = wait_for(Duration::from_secs(10), || async {
      search_code(&handle, "delete_function")
        .await
        .map(|r| r.chunks.is_empty() || !r.chunks.iter().any(|c| c.content.contains("delete_function")))
        .unwrap_or(false)
    })
    .await;

    assert!(removed, "Watcher should remove deleted file from index");

    // Kept file should still be there
    let search = search_code(&handle, "keep_function").await.expect("search");
    assert!(!search.chunks.is_empty(), "Kept file should remain indexed");

    cancel.cancel();
  }

  /// Test: Watcher -> rename file -> path updated, content preserved.
  ///
  /// Note: On some platforms, rename events come as Delete+Create pairs rather
  /// than a single Rename event. In that case, the file gets reindexed with the
  /// new path. Either way, the content should remain searchable with the new path.
  #[tokio::test]
  async fn test_watcher_handles_rename() {
    let ctx = ActorTestContext::new().await;

    // Create and index file
    ctx
      .write_source_file("src/old_live.rs", "pub fn live_rename() {}")
      .await;

    let (handle, cancel) = ctx.spawn_project_actor().await.expect("spawn actor");
    trigger_index(&handle).await.expect("index should succeed");

    // Start watcher
    start_watcher(&handle).await.expect("start watcher");
    wait_for_scan_complete(&handle, Duration::from_secs(5)).await;

    // Rename file
    ctx.rename_source_file("src/old_live.rs", "src/new_live.rs").await;

    // Wait for watcher to process rename
    // On some platforms this is a Rename event, on others it's Delete+Create
    // Either way, we should eventually have the content at the new path
    let rename_complete = wait_for(Duration::from_secs(15), || async {
      if let Ok(search) = search_code(&handle, "live_rename").await {
        // Check if any chunk has the new path AND old path is gone
        let has_new_path = search.chunks.iter().any(|c| c.file_path.contains("new_live"));
        let old_gone = !search.chunks.iter().any(|c| c.file_path.contains("old_live"));
        has_new_path && old_gone
      } else {
        false
      }
    })
    .await;

    assert!(rename_complete, "Rename should update path from old_live to new_live");

    cancel.cancel();
  }

  // ==========================================================================
  // Touch File / Mtime-Only Change Tests
  // ==========================================================================

  /// Test: Touch file (mtime change only) -> startup scan detects change -> content hash prevents redundant reindex.
  ///
  /// This tests the mtime-based change detection. When a file's mtime changes but content
  /// doesn't, the startup scan should detect the mtime difference but content hashing
  /// should prevent unnecessary re-embedding.
  #[tokio::test]
  async fn test_startup_scan_handles_touch_only_change() {
    let ctx = ActorTestContext::new().await;

    // Create and index a file
    ctx.write_source_file("src/lib.rs", "pub fn stable_content() {}").await;

    let (handle1, cancel1) = ctx.spawn_project_actor().await.expect("spawn actor");
    let index_result = trigger_index(&handle1).await.expect("index should succeed");
    assert!(index_result.files_indexed > 0, "Should index at least one file");

    // Verify content is searchable
    let search = search_code(&handle1, "stable_content").await.expect("search");
    assert!(!search.chunks.is_empty(), "Should find content before touch");

    // Stop the actor
    cancel1.cancel();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Touch the file (changes mtime but not content)
    ctx.touch_file("src/lib.rs");

    // Restart and start watcher (triggers startup scan)
    let (handle2, cancel2) = ctx.spawn_project_actor().await.expect("spawn actor");
    start_watcher(&handle2).await.expect("start watcher");
    wait_for_scan_complete(&handle2, Duration::from_secs(10)).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Content should still be searchable (not broken by the touch)
    let search = search_code(&handle2, "stable_content").await.expect("search");
    assert!(
      !search.chunks.is_empty(),
      "Content should remain searchable after touch-only change"
    );

    cancel2.cancel();
  }

  // ==========================================================================
  // Watcher Stop/Start Lifecycle Tests
  // ==========================================================================

  /// Test: Stop watcher -> file changes -> changes not detected until watcher restarted.
  #[tokio::test]
  async fn test_watcher_stop_prevents_change_detection() {
    let ctx = ActorTestContext::new().await;

    // Create and index initial file
    ctx.write_source_file("src/lib.rs", "pub fn initial() {}").await;

    let (handle, cancel) = ctx.spawn_project_actor().await.expect("spawn actor");
    trigger_index(&handle).await.expect("index should succeed");

    // Start watcher
    start_watcher(&handle).await.expect("start watcher");
    wait_for_scan_complete(&handle, Duration::from_secs(5)).await;

    // Stop the watcher
    stop_watcher(&handle).await.expect("stop watcher");
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create a new file while watcher is stopped
    ctx
      .write_source_file("src/while_stopped.rs", "pub fn created_while_stopped() {}")
      .await;

    // Wait a bit - the file should NOT be indexed
    tokio::time::sleep(Duration::from_millis(500)).await;

    let search = search_code(&handle, "created_while_stopped").await.expect("search");
    // Semantic search may return false positives, so we check if ANY result contains the actual content
    assert!(
      search.chunks.is_empty()
        || !search
          .chunks
          .iter()
          .any(|c| c.content.contains("created_while_stopped")),
      "File created while watcher stopped should not be indexed"
    );

    // Restart watcher - should pick up the file via startup scan
    start_watcher(&handle).await.expect("restart watcher");
    wait_for_scan_complete(&handle, Duration::from_secs(10)).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Now the file should be indexed
    let search = search_code(&handle, "created_while_stopped").await.expect("search");
    assert!(
      !search.chunks.is_empty(),
      "File should be indexed after watcher restart"
    );

    cancel.cancel();
  }
}
