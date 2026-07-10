pub mod policy;
pub mod store;

// Re-export so users can just use `kv_store::KvStore` instead of `kv_store::store::KvStore`
pub use policy::*;
pub use store::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    // ==========================================
    // 1. STRATEGY TESTS (The Happy Paths)
    // ==========================================

    #[test]
    fn test_fifo_strategy() {
        let mut store = KvStore::new(2, FifoPolicy::new());
        
        store.set("k1".to_string(), "v1".to_string(), None);
        store.set("k2".to_string(), "v2".to_string(), None);
        // k1 is oldest. Inserting k3 should evict k1.
        store.set("k3".to_string(), "v3".to_string(), None);

        assert_eq!(store.get("k1"), None, "k1 should have been evicted via FIFO");
        assert_eq!(store.get("k2"), Some("v2".to_string()));
        assert_eq!(store.get("k3"), Some("v3".to_string()));
    }

    #[test]
    fn test_lru_strategy() {
        let mut store = KvStore::new(2, LruPolicy::new());
        
        store.set("A".to_string(), "1".to_string(), None);
        store.set("B".to_string(), "2".to_string(), None);
        
        // Access A, making B the least recently used
        store.get("A");
        
        // Insert C. B should be evicted because it is LRU.
        store.set("C".to_string(), "3".to_string(), None);

        assert_eq!(store.get("B"), None, "B should have been evicted via LRU");
        assert_eq!(store.get("A"), Some("1".to_string()));
        assert_eq!(store.get("C"), Some("3".to_string()));
    }

    #[test]
    fn test_lfu_strategy() {
        let mut store = KvStore::new(2, LfuPolicy::new());
        
        store.set("X".to_string(), "10".to_string(), None);
        store.set("Y".to_string(), "20".to_string(), None);
        
        // Access Y twice (Frequency = 3: 1 insert + 2 gets)
        store.get("Y");
        store.get("Y");
        
        // Access X once (Frequency = 2: 1 insert + 1 get)
        store.get("X");

        // Insert Z. X has lower frequency (2) than Y (3), so X is evicted.
        store.set("Z".to_string(), "30".to_string(), None);

        assert_eq!(store.get("X"), None, "X should have been evicted via LFU");
        assert_eq!(store.get("Y"), Some("20".to_string()));
        assert_eq!(store.get("Z"), Some("30".to_string()));
    }

    // ==========================================
    // 2. EDGE CASE TESTS
    // ==========================================

    #[test]
    fn test_edge_capacity_one() {
        // A cache with a capacity of exactly 1 is a great edge case
        // It basically acts as a single-value holding cell
        let mut store = KvStore::new(1, FifoPolicy::new());
        
        store.set("Mihir".to_string(), "Data Engineer".to_string(), None);
        assert_eq!(store.get("Mihir"), Some("Data Engineer".to_string()));

        // Setting a new key should instantly wipe the only existing key
        store.set("Rust".to_string(), "Awesome".to_string(), None);
        assert_eq!(store.get("Mihir"), None);
        assert_eq!(store.get("Rust"), Some("Awesome".to_string()));
    }

    #[test]
    fn test_edge_update_existing_key_lru() {
        let mut store = KvStore::new(2, LruPolicy::new());
        
        store.set("k1".to_string(), "v1".to_string(), None);
        store.set("k2".to_string(), "v2".to_string(), None);
        
        // Overwriting an existing key counts as an access in our implementation!
        // This should bump k1 to the most recently used spot.
        store.set("k1".to_string(), "v1_updated".to_string(), None);
        
        // Insert k3. Because k1 was just updated, k2 is now the LRU.
        store.set("k3".to_string(), "v3".to_string(), None);

        assert_eq!(store.get("k2"), None, "k2 should be evicted after k1 was updated");
        assert_eq!(store.get("k1"), Some("v1_updated".to_string()));
    }

    #[test]
    fn test_edge_hybrid_ttl_lazy_eviction() {
        let mut store = KvStore::new(10, LruPolicy::new());
        
        // Insert with a 10-millisecond TTL
        store.set("temp".to_string(), "data".to_string(), Some(Duration::from_millis(10)));
        
        // It should exist immediately
        assert_eq!(store.get("temp"), Some("data".to_string()));
        
        // Sleep the thread for 15 milliseconds to force expiration
        sleep(Duration::from_millis(15));
        
        // The lazy evaluation should catch the dead key and return None
        assert_eq!(store.get("temp"), None, "TTL expired key should be lazily deleted");
    }

    #[test]
    fn test_edge_empty_strings() {
        // Edge cases with no characters to ensure hashing and lookup don't crash
        let mut store = KvStore::new(5, LfuPolicy::new());
        
        store.set("".to_string(), "".to_string(), None);
        assert_eq!(store.get(""), Some("".to_string()));
    }

    #[test]
    fn test_edge_manual_delete_handling() {
        let mut store = KvStore::new(2, FifoPolicy::new());
        
        store.set("k1".to_string(), "v1".to_string(), None);
        
        // Manually delete the key
        store.delete("k1");
        
        // Assert it is gone
        assert_eq!(store.get("k1"), None);
        
        // Ensure that filling the cache again doesn't crash the policy
        // even though the policy might have "k1" floating in its queue as a ghost
        store.set("k2".to_string(), "v2".to_string(), None);
        store.set("k3".to_string(), "v3".to_string(), None);
        store.set("k4".to_string(), "v4".to_string(), None); // Triggers eviction
        
        assert_eq!(store.get("k4"), Some("v4".to_string()));
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        // 1. Initialize the thread-safe store with a large capacity
        // Notice we don't need `mut` here! The RwLock handles internal mutability.
        let store = ConcurrentKvStore::new(2000, FifoPolicy::new());
        
        // A vector to hold the "handles" (controls) for our spawned threads
        let mut thread_handles = vec![];

        // 2. Spawn 10 simultaneous threads
        for thread_id in 0..10 {
            // THE MAGIC TRICK: We clone the store. 
            // Because it's wrapped in an `Arc`, this DOES NOT copy the database. 
            // It just increments the reference count from 1 to 2, 2 to 3, etc.
            let thread_store = store.clone(); 
            
            // `thread::spawn` creates a real OS thread.
            // `move` tells the thread to take full ownership of its specific `thread_store` clone.
            let handle = thread::spawn(move || {
                
                // Each thread fires 100 distinct write operations
                for i in 0..100 {
                    let key = format!("agent_{}_token_{}", thread_id, i);
                    let value = format!("active_time_{}", i);
                    
                    // The RwLock inside `.set()` automatically forces these threads 
                    // to line up gracefully without corrupting the HashMap
                    thread_store.set(key, value, None);
                }
            });
            
            thread_handles.push(handle);
        }

        // 3. Wait for the chaos to finish
        // We iterate through all 10 threads and tell the main test thread to pause
        // until every single spawned thread has completed its work.
        for handle in thread_handles {
            handle.join().unwrap();
        }

        // 4. Verify the data survived the concurrent onslaught
        // We randomly check a few keys to prove they were successfully written and retained
        assert_eq!(
            store.get("agent_3_token_50"), 
            Some("active_time_50".to_string())
        );
        assert_eq!(
            store.get("agent_9_token_99"), 
            Some("active_time_99".to_string())
        );
        assert_eq!(
            store.get("agent_0_token_0"), 
            Some("active_time_0".to_string())
        );
    }
}