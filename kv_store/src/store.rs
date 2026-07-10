use std::collections::HashMap;
// Bring in the threading primitives (Arc = shared ownership, RwLock = access control)
use std::sync::{Arc, RwLock};
// Bring in time modules for our Lazy TTL implementation
use std::time::{Duration, Instant};
// Import the trait we just defined in the other file
use crate::policy::EvictionPolicy;

// ==========================================
// PHASE 1: THE CORE ENGINE
// ==========================================

// A wrapper around our value that also holds its "Death Time" (expires_at).
pub struct CacheEntry {
    pub value: String,
    pub expires_at: Option<Instant>, // Option because some keys live forever (None)
}

// `<P: EvictionPolicy>` tells Rust: "This struct takes a generic type P, 
// but P MUST implement the EvictionPolicy trait."
pub struct KvStore<P: EvictionPolicy> {
    store: HashMap<String, CacheEntry>,
    capacity: usize, // The maximum number of keys allowed
    policy: P,       // The injected tracking algorithm (FIFO, LRU, etc.)
}

impl<P: EvictionPolicy> KvStore<P> {
    pub fn new(capacity: usize, policy: P) -> Self {
        Self {
            store: HashMap::new(),
            capacity,
            policy,
        }
    }

    pub fn set(&mut self, key: String, value: String, ttl: Option<Duration>) {
        // Calculate the exact timestamp when this key should die.
        let expires_at = ttl.map(|dur| Instant::now() + dur);

        // 1. Capacity Check: If the store is full AND this is a brand new key...
        if !self.store.contains_key(&key) && self.store.len() >= self.capacity {
            // Ask the policy who to kill, then remove them from the HashMap.
            if let Some(victim) = self.policy.evict() {
                self.store.remove(&victim);
            }
        }

        // 2. Policy Update: Tell the policy what we are doing.
        if self.store.contains_key(&key) {
            self.policy.on_access(&key);
        } else {
            // `.clone()` because we need to give ownership to the policy, 
            // but we still need the original `key` to put in the HashMap below.
            self.policy.on_insert(key.clone());
        }

        // 3. Actual Storage
        self.store.insert(key, CacheEntry { value, expires_at });
    }

    pub fn get(&mut self, key: &str) -> Option<String> {
        // 1. Lazy TTL Evaluation
        let mut is_expired = false;
        
        // Peek at the entry to see if the time has run out
        if let Some(entry) = self.store.get(key) {
            if let Some(expiry) = entry.expires_at {
                if Instant::now() > expiry {
                    is_expired = true;
                }
            }
        }

        // If time ran out, delete it immediately and return a Cache Miss (None)
        if is_expired {
            self.store.remove(key);
            return None;
        }

        // 2. If it is alive, update the policy and return the value
        if let Some(entry) = self.store.get(key) {
            self.policy.on_access(key);
            Some(entry.value.clone())
        } else {
            None
        }
    }

    pub fn delete(&mut self, key: &str) {
        self.store.remove(key);
    }
}

// ==========================================
// PHASE 2: THE CONCURRENT WRAPPER
// ==========================================

// This is the shield that protects KvStore from Threading Data Races.
pub struct ConcurrentKvStore<P: EvictionPolicy> {
    // Arc = Allows multiple threads to hold a pointer to this data.
    // RwLock = Allows infinite readers, or ONE exclusive writer.
    inner: Arc<RwLock<KvStore<P>>>,
}

// We manually implement `Clone` so that when we spawn a new thread, 
// we only clone the Arc pointer (incrementing the reference count), 
// we DO NOT copy the entire database.
impl<P: EvictionPolicy> Clone for ConcurrentKvStore<P> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<P: EvictionPolicy> ConcurrentKvStore<P> {
    pub fn new(capacity: usize, policy: P) -> Self {
        let single_threaded_store = KvStore::new(capacity, policy);
        Self {
            inner: Arc::new(RwLock::new(single_threaded_store)),
        }
    }

    // Notice we only need `&self` (immutable reference) here! 
    // The RwLock provides "interior mutability", allowing us to modify 
    // the inside safely even if the outside is immutable.
    pub fn set(&self, key: String, value: String, ttl: Option<Duration>) {
        // `.write()` asks the lock for exclusive access. 
        // If someone else is reading/writing, this thread pauses here and waits.
        // `.unwrap()` says: "If the lock crashed, crash the program."
        let mut store_lock = self.inner.write().unwrap();
        
        // Now that we have the lock, call the single-threaded method safely.
        store_lock.set(key, value, ttl);
    }

    pub fn get(&self, key: &str) -> Option<String> {
        // We MUST use a write lock for `get` because reading a key 
        // updates the LRU/LFU tracking logic, which modifies state!
        let mut store_lock = self.inner.write().unwrap();
        store_lock.get(key)
    }

    pub fn delete(&self, key: &str) {
        let mut store_lock = self.inner.write().unwrap();
        store_lock.delete(key);
    }
}