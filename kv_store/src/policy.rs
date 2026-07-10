// std::collections brings in standard data structures. 
// VecDeque is a double-ended queue (fast pushes/pops from both the front and back).
use std::collections::{HashMap, VecDeque};

// ==========================================
// THE TRAIT (The Interface)
// ==========================================
// A trait defines a shared contract. Any struct that wants to act as an 
// eviction policy MUST implement these three exact methods.
pub trait EvictionPolicy {
    // `&mut self` means the function will modify the policy's internal state.
    // `String` means the policy takes full ownership of the key.
    fn on_insert(&mut self, key: String);
    
    // `&str` means the policy only needs to *look* at the key (borrow it), 
    // it doesn't need to own it.
    fn on_access(&mut self, key: &str);
    
    // Returns an `Option<String>`. It might return `Some(key)` to delete, 
    // or `None` if the queue is mysteriously empty.
    fn evict(&mut self) -> Option<String>;
}

// ==========================================
// FIFO POLICY
// ==========================================
pub struct FifoPolicy {
    queue: VecDeque<String>, // Tracks insertion order
}

impl FifoPolicy {
    // The constructor. `Self` is a shorthand for `FifoPolicy`.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }
}

// We implement the trait contract for the FifoPolicy struct.
impl EvictionPolicy for FifoPolicy {
    fn on_insert(&mut self, key: String) {
        // New keys go to the back of the line.
        self.queue.push_back(key);
    }

    fn on_access(&mut self, _key: &str) {
        // The `_` in `_key` tells the Rust compiler: "I know I'm not using 
        // this variable, please don't give me a warning about it."
        // FIFO doesn't care how often you read a key.
    }

    fn evict(&mut self) -> Option<String> {
        // Pops and returns the oldest key from the front of the line.
        self.queue.pop_front()
    }
}

// ==========================================
// LRU POLICY (Least Recently Used)
// ==========================================
pub struct LruPolicy {
    queue: VecDeque<String>, // Tracks access order
}

impl LruPolicy {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }
}

impl EvictionPolicy for LruPolicy {
    fn on_insert(&mut self, key: String) {
        self.queue.push_back(key);
    }

    fn on_access(&mut self, key: &str) {
        // 1. `.iter().position(...)` scans the queue to find where our key is.
        if let Some(pos) = self.queue.iter().position(|k| k == key) {
            // 2. We remove it from its current spot in the middle of the queue.
            // `.unwrap()` is safe here because we just confirmed it exists.
            let k = self.queue.remove(pos).unwrap();
            
            // 3. We push it to the very back, marking it as "Most Recently Used".
            self.queue.push_back(k);
        }
    }

    fn evict(&mut self) -> Option<String> {
        // The front of the queue is naturally the "Least Recently Used".
        self.queue.pop_front()
    }
}

// ==========================================
// LFU POLICY (Least Frequently Used)
// ==========================================
pub struct LfuPolicy {
    frequencies: HashMap<String, usize>, // Maps a Key to its Hit Count
}

impl LfuPolicy {
    pub fn new() -> Self {
        Self {
            frequencies: HashMap::new(),
        }
    }
}

impl EvictionPolicy for LfuPolicy {
    fn on_insert(&mut self, key: String) {
        // Start the hit count at 1.
        self.frequencies.insert(key, 1);
    }

    fn on_access(&mut self, key: &str) {
        // `.get_mut()` gets a mutable reference to the value inside the map.
        if let Some(count) = self.frequencies.get_mut(key) {
            // The `*` dereferences the pointer so we can add 1 to the actual number.
            *count += 1;
        }
    }

    fn evict(&mut self) -> Option<String> {
        // 1. Iterate over the whole map.
        // 2. `.min_by_key` finds the entry with the absolute lowest count.
        // 3. `.map` extracts just the key string, cloning it so we own it.
        let victim = self.frequencies
            .iter()
            .min_by_key(|&(_, count)| count)
            .map(|(key, _)| key.clone());

        // 4. If we found a victim, remove it from our tracking map.
        if let Some(ref key) = victim {
            self.frequencies.remove(key);
        }
        
        victim // Return the key to the main store so it can be deleted.
    }
}