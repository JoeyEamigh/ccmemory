#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use tokio::sync::mpsc;
  use tokio_util::sync::CancellationToken;

  use crate::{
    actor::{handle::IndexerHandle, message::IndexJob, watcher::*},
    domain::config::IndexConfig,
  };

  #[tokio::test]
  async fn test_watcher_task_integration() {
    use std::fs;

    use tokio::time::{Duration, sleep, timeout};

    // Create a temporary directory for testing
    let temp_dir = std::env::temp_dir().join(format!("watcher_test_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("create temp dir");

    // Ensure cleanup on drop
    struct TempDirGuard(PathBuf);
    impl Drop for TempDirGuard {
      fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
      }
    }
    let _guard = TempDirGuard(temp_dir.clone());

    // Create IndexerHandle and WatcherTask
    let (tx, mut rx) = mpsc::channel::<IndexJob>(100);
    let handle = IndexerHandle::new(tx);
    let cancel = CancellationToken::new();

    // Create config with short debounce for testing
    let index_config = IndexConfig {
      watcher_debounce_ms: 50,
      ..Default::default()
    };
    let config = WatcherConfig {
      root: temp_dir.clone(),
      index: index_config,
    };

    let watcher = WatcherTask::new(config, handle, cancel.clone()).expect("create watcher");

    // Spawn the watcher task
    let watcher_task = tokio::spawn(watcher.run());

    // Give watcher time to initialize
    sleep(Duration::from_millis(100)).await;

    // Test 1: Create a file
    let test_file = temp_dir.join("test.rs");
    fs::write(&test_file, "fn main() {}").expect("write file");

    // Wait for and verify the create event
    let job = timeout(Duration::from_secs(2), rx.recv())
      .await
      .expect("timeout waiting for create event")
      .expect("receive create event");

    match job {
      IndexJob::File { path, old_content } => {
        assert_eq!(path, test_file);
        assert!(old_content.is_none());
      }
      other => panic!("expected IndexJob::File for create, got {:?}", other),
    }

    // Test 2: Modify the file
    fs::write(&test_file, "fn main() { println!(\"hello\"); }").expect("modify file");

    let job = timeout(Duration::from_secs(2), rx.recv())
      .await
      .expect("timeout waiting for modify event")
      .expect("receive modify event");

    match job {
      IndexJob::File { path, old_content } => {
        assert_eq!(path, test_file);
        // Should have cached the old content
        assert!(old_content.is_some());
      }
      other => panic!("expected IndexJob::File for modify, got {:?}", other),
    }

    // Test 3: Delete the file
    fs::remove_file(&test_file).expect("delete file");

    let job = timeout(Duration::from_secs(2), rx.recv())
      .await
      .expect("timeout waiting for delete event")
      .expect("receive delete event");

    match job {
      IndexJob::Delete { path } => {
        assert_eq!(path, test_file);
      }
      other => panic!("expected IndexJob::Delete, got {:?}", other),
    }

    // Test 4: Rename operation
    let file1 = temp_dir.join("old.rs");
    let file2 = temp_dir.join("new.rs");
    fs::write(&file1, "fn old() {}").expect("write old file");

    // Wait for create event
    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    // Perform rename
    fs::rename(&file1, &file2).expect("rename file");

    // Should get either a Rename event or a Delete+Create pair
    let job = timeout(Duration::from_secs(2), rx.recv())
      .await
      .expect("timeout waiting for rename event")
      .expect("receive rename event");

    match job {
      IndexJob::Rename { from, to } => {
        assert_eq!(from, file1);
        assert_eq!(to, file2);
      }
      IndexJob::Delete { path } => {
        // On some platforms we get delete+create instead
        assert_eq!(path, file1);
        let job2 = timeout(Duration::from_secs(2), rx.recv())
          .await
          .expect("get create")
          .expect("receive");
        match job2 {
          IndexJob::File { path, .. } => assert_eq!(path, file2),
          other => panic!("expected File after Delete, got {:?}", other),
        }
      }
      other => panic!("expected Rename or Delete, got {:?}", other),
    }

    // Cleanup: cancel the watcher
    cancel.cancel();
    let _ = timeout(Duration::from_secs(2), watcher_task)
      .await
      .expect("watcher task should stop");
  }
}
