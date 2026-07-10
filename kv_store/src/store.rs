use std::collections::HashMap;
use std::hash::Hash;
// Bring in the threading primitives (Arc = shared ownership, RwLock = data access, Mutex = policy lock)
use std::sync::{Arc, Mutex, RwLock};
// Bring in time modules for our Lazy TTL implementation
use std::time::{Duration, Instant};
// Import the trait we just defined in the other file
use crate::policy::EvictionPolicy;

// ==========================================
// PHASE 1: THE CORE ENGINE
// ==========================================

// A wrapper around our value that also holds its "Death Time" (expires_at).
// Now generic over `V` so it can hold any value type, not just String.
pub struct CacheEntry<V> {
    pub value: V,
    pub expires_at: Option<Instant>, // Option because some keys live forever (None)
}

// `<K, V, P>` tells Rust: "This struct takes three generic types."
// K must be usable as a HashMap key (Eq + Hash) and cloneable (for the policy).
// V must be cloneable (because get() returns a copy).
// P must implement our EvictionPolicy trait for key type K.
pub struct KvStore<K, V, P>
where
    K: Eq + Hash + Clone,
    V: Clone,
    P: EvictionPolicy<K>,
{
    store: HashMap<K, CacheEntry<V>>,
    capacity: usize, // The maximum number of keys allowed
    policy: P,       // The injected tracking algorithm (FIFO, LRU, etc.)
}

impl<K, V, P> KvStore<K, V, P>
where
    K: Eq + Hash + Clone,
    V: Clone,
    P: EvictionPolicy<K>,
{
    pub fn new(capacity: usize, policy: P) -> Self {
        Self {
            store: HashMap::new(),
            capacity,
            policy,
        }
    }

    pub fn set(&mut self, key: K, value: V, ttl: Option<Duration>) {
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

    pub fn get(&mut self, key: &K) -> Option<V> {
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
            self.policy.on_delete(key);
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

    pub fn delete(&mut self, key: &K) {
        self.store.remove(key);
        self.policy.on_delete(key);
    }
}

// ==========================================
// PHASE 2: THE CONCURRENT WRAPPER (Split-Lock Architecture)
// ==========================================
//
// REDESIGNED: Instead of wrapping the entire KvStore in a single RwLock
// (which forced ALL operations — including reads — to serialize),
// we now separate the DATA PLANE from the CONTROL PLANE:
//
//   Data Plane:    RwLock<HashMap<K, CacheEntry<V>>>   → many concurrent readers
//   Control Plane: Mutex<P>                             → serialized policy updates
//
// This means get() can read the HashMap concurrently with other readers,
// only briefly locking the Mutex to update the policy's access tracking.
//
// LOCK ORDERING (Deadlock Prevention):
// All methods that acquire both locks MUST acquire them in this order:
//   1. policy (Mutex)  FIRST
//   2. data (RwLock)   SECOND
// The get() fast path avoids this issue entirely because it releases
// the read lock before acquiring the policy mutex.

pub struct ConcurrentKvStore<K, V, P>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    P: EvictionPolicy<K> + Send + 'static,
{
    // The DATA PLANE: holds the actual key-value entries.
    // RwLock allows infinite concurrent readers, or ONE exclusive writer.
    data: Arc<RwLock<HashMap<K, CacheEntry<V>>>>,

    // The CONTROL PLANE: holds the eviction policy's tracking structures.
    // Mutex provides exclusive access for policy mutations.
    policy: Arc<Mutex<P>>,

    // The maximum number of keys allowed in the store.
    capacity: usize,
}

// We manually implement `Clone` so that when we spawn a new thread,
// we only clone the Arc pointers (incrementing the reference counts),
// we DO NOT copy the database or the policy.
impl<K, V, P> Clone for ConcurrentKvStore<K, V, P>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    P: EvictionPolicy<K> + Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
            policy: Arc::clone(&self.policy),
            capacity: self.capacity,
        }
    }
}

impl<K, V, P> ConcurrentKvStore<K, V, P>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    P: EvictionPolicy<K> + Send + 'static,
{
    pub fn new(capacity: usize, policy: P) -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            policy: Arc::new(Mutex::new(policy)),
            capacity,
        }
    }

    // ── GET: The Big Win ─────────────────────────────────────────
    // The read path is split into two small critical sections:
    //   Phase 1: data.read()   → shared lock, concurrent with all other readers
    //   Phase 2: policy.lock() → brief exclusive lock for access tracking
    //
    // Multiple threads can read the HashMap simultaneously in Phase 1.
    // They only briefly serialize during Phase 2 (policy bookkeeping).
    pub fn get(&self, key: &K) -> Option<V> {
        // PHASE 1: Read-lock only (CONCURRENT with all other readers!)
        let (value, is_expired) = {
            let data = self.data.read().unwrap();
            match data.get(key) {
                Some(entry) => {
                    let expired = entry.expires_at
                        .map_or(false, |exp| Instant::now() > exp);
                    if expired {
                        (None, true)
                    } else {
                        (Some(entry.value.clone()), false)
                    }
                }
                None => (None, false),
            }
        }; // ← read lock released here

        // PHASE 2: Expired key cleanup (rare slow path)
        // Lock ordering: policy → data
        if is_expired {
            let mut policy = self.policy.lock().unwrap();
            let mut data = self.data.write().unwrap();
            data.remove(key);
            policy.on_delete(key);
            return None;
        }

        // PHASE 3: Policy bookkeeping (brief exclusive lock on policy ONLY)
        if value.is_some() {
            let mut policy = self.policy.lock().unwrap();
            policy.on_access(key);
        }

        value
    }

    // ── SET: Both Locks Held for Atomicity ────────────────────────
    // Write operations need atomicity between the eviction decision
    // and the data modification, so we hold both locks simultaneously.
    // Lock ordering: policy FIRST → data SECOND (prevents deadlocks).
    pub fn set(&self, key: K, value: V, ttl: Option<Duration>) {
        let expires_at = ttl.map(|dur| Instant::now() + dur);

        // Lock ordering: policy → data
        let mut policy = self.policy.lock().unwrap();
        let mut data = self.data.write().unwrap();

        // Capacity check + eviction
        if !data.contains_key(&key) && data.len() >= self.capacity {
            if let Some(victim) = policy.evict() {
                data.remove(&victim);
            }
        }

        // Policy update
        if data.contains_key(&key) {
            policy.on_access(&key);
        } else {
            policy.on_insert(key.clone());
        }

        // Actual storage
        data.insert(key, CacheEntry { value, expires_at });
    }

    // ── DELETE: Same Lock Ordering ────────────────────────────────
    pub fn delete(&self, key: &K) {
        // Lock ordering: policy → data
        let mut policy = self.policy.lock().unwrap();
        let mut data = self.data.write().unwrap();
        data.remove(key);
        policy.on_delete(key);
    }
}