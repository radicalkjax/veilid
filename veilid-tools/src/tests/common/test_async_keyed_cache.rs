use crate::*;

pub async fn test_get_insert_remove() {
    info!("test_get_insert_remove");

    let cache = AsyncKeyedCache::<u32, String>::new(16);

    let entry = cache.lookup(1).await;
    assert_eq!(entry.get(), None);
    assert_eq!(entry.insert("one".to_string()), None);
    assert_eq!(entry.get(), Some("one".to_string()));
    // insert returns the previous value
    assert_eq!(entry.insert("uno".to_string()), Some("one".to_string()));
    assert_eq!(entry.remove(), Some("uno".to_string()));
    assert_eq!(entry.get(), None);
}

pub async fn test_same_key_serializes() {
    info!("test_same_key_serializes");

    let cache = AsyncKeyedCache::<u32, u32>::new(16);

    // First task locks key 1, fills it after a delay, then releases.
    let cache1 = cache.clone();
    let t1 = spawn("t1", async move {
        let entry = cache1.lookup(1).await;
        info!("t1 locked key 1");
        sleep(500).await;
        entry.insert(100);
        info!("t1 inserted and releasing");
        // entry (and its lock) released on drop here
    });

    // Let t1 acquire the lock first.
    sleep(100).await;

    // This lookup of key 1 must wait until t1 releases; by then the value is set.
    let entry = cache.lookup(1).await;
    assert_eq!(
        entry.get(),
        Some(100),
        "same-key lookup must serialize behind t1 and see its insert"
    );

    t1.await;
}

pub async fn test_different_keys_dont_block() {
    info!("test_different_keys_dont_block");

    let cache = AsyncKeyedCache::<u32, ()>::new(16);

    // Hold key 1 for the duration.
    let _held = cache.lookup(1).await;

    // A different key must proceed immediately (this would hang if it blocked).
    let other = cache.lookup(2).await;
    other.insert(());
    assert_eq!(other.get(), Some(()));
}

pub async fn test_all() {
    test_get_insert_remove().await;
    test_same_key_serializes().await;
    test_different_keys_dont_block().await;
}
