// std::collections brings in standard data structures.
// VecDeque is a double-ended queue (fast pushes/pops from both the front and back).
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

// ==========================================
// THE TRAIT (The Interface)
// ==========================================
// A trait defines a shared contract. Any struct that wants to act as an
// eviction policy MUST implement these four exact methods.
// The generic `K` represents the key type. It must be Eq (comparable),
// Hash (hashable), and Clone (copyable) because policies need to
// store, compare, and return owned copies of keys.
pub trait EvictionPolicy<K: Eq + Hash + Clone> {
    // `&mut self` means the function will modify the policy's internal state.
    // `K` means the policy takes full ownership of the key.
    fn on_insert(&mut self, key: K);

    // `&K` means the policy only needs to *look* at the key (borrow it),
    // it doesn't need to own it.
    fn on_access(&mut self, key: &K);

    // Returns an `Option<K>`. It might return `Some(key)` to delete,
    // or `None` if the queue is mysteriously empty.
    fn evict(&mut self) -> Option<K>;

    // Called when a key is explicitly deleted or TTL-expired.
    // The policy MUST remove the key from its internal tracking structures
    // to prevent "ghost keys" from polluting future eviction decisions.
    fn on_delete(&mut self, key: &K);
}

// ==========================================
// FIFO POLICY
// ==========================================
// FIFO stays VecDeque-based because it's already O(1) for insert and evict.
// The only O(N) operation is on_delete, which is acceptable since manual
// deletes are rare in cache workloads and FIFO's simplicity is its feature.
pub struct FifoPolicy<K> {
    queue: VecDeque<K>, // Tracks insertion order
}

impl<K: Eq + Hash + Clone> FifoPolicy<K> {
    // The constructor. `Self` is a shorthand for `FifoPolicy<K>`.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }
}

// We implement the trait contract for the FifoPolicy struct.
impl<K: Eq + Hash + Clone> EvictionPolicy<K> for FifoPolicy<K> {
    fn on_insert(&mut self, key: K) {
        // New keys go to the back of the line.
        self.queue.push_back(key);
    }

    fn on_access(&mut self, _key: &K) {
        // The `_` in `_key` tells the Rust compiler: "I know I'm not using
        // this variable, please don't give me a warning about it."
        // FIFO doesn't care how often you read a key.
    }

    fn evict(&mut self) -> Option<K> {
        // Pops and returns the oldest key from the front of the line.
        self.queue.pop_front()
    }

    fn on_delete(&mut self, key: &K) {
        // Scan the queue to find and remove the deleted key.
        // O(N) but acceptable since manual deletes are rare in cache workloads.
        if let Some(pos) = self.queue.iter().position(|k| k == key) {
            self.queue.remove(pos);
        }
    }
}

// ==========================================
// LRU POLICY (Arena-Based Doubly Linked List)
// ==========================================
//
// THE ARENA PATTERN:
// In most languages, a doubly linked list uses heap-allocated nodes with
// pointers. In Rust, this is painful because shared mutable pointers
// (each node's prev AND next both point into the same list) violate
// the ownership model without `unsafe`.
//
// The Arena trick: instead of pointers, we store all nodes in a Vec
// (the "arena") and reference them by INDEX (usize). Indices are just
// numbers — they're Copy, they don't borrow anything, and the borrow
// checker is happy. We get the same O(1) random access as pointers,
// but in 100% safe Rust.
//
// Memory recycling: When a node is deleted, its index goes into a
// `free_list`. The next allocation reuses a free slot instead of
// growing the Vec. This prevents unbounded arena growth.
//
// COMPLEXITY: O(1) for ALL operations (insert, access, evict, delete).

/// A single node in the LRU doubly linked list, stored in the arena.
struct LruNode<K> {
    key: K,
    prev: Option<usize>, // Index of the previous node (toward the tail/LRU end)
    next: Option<usize>, // Index of the next node (toward the head/MRU end)
}

pub struct LruPolicy<K> {
    // The arena: all nodes live here, referenced by index.
    // `Option<LruNode>` because a slot can be empty (freed/recycled).
    arena: Vec<Option<LruNode<K>>>,

    // O(1) lookup: given a key, find its index in the arena.
    key_to_index: HashMap<K, usize>,

    // The doubly linked list's endpoints:
    head: Option<usize>, // Most Recently Used (front of the list)
    tail: Option<usize>, // Least Recently Used (back of the list, eviction target)

    // Recycled arena slots available for reuse.
    free_list: Vec<usize>,
}

impl<K: Eq + Hash + Clone> LruPolicy<K> {
    pub fn new() -> Self {
        Self {
            arena: Vec::new(),
            key_to_index: HashMap::new(),
            head: None,
            tail: None,
            free_list: Vec::new(),
        }
    }

    // ── Arena Allocator ──────────────────────────────────────────
    // Reuses a freed slot if available, otherwise grows the Vec.
    fn alloc(&mut self) -> usize {
        if let Some(idx) = self.free_list.pop() {
            idx
        } else {
            let idx = self.arena.len();
            self.arena.push(None); // Reserve the slot
            idx
        }
    }

    // ── DLL Operation: Link at Head ──────────────────────────────
    // Inserts a node at the HEAD (Most Recently Used) position.
    //
    //   Before:  HEAD ←→ [old_head] ←→ ... ←→ [tail]
    //   After:   HEAD ←→ [new_node] ←→ [old_head] ←→ ... ←→ [tail]
    //
    fn link_at_head(&mut self, idx: usize) {
        let old_head = self.head;

        // Point the new node forward to the old head, backward to nothing
        if let Some(node) = self.arena[idx].as_mut() {
            node.prev = None;
            node.next = old_head;
        }

        // Tell the old head that someone is now in front of it
        if let Some(old_h) = old_head {
            if let Some(old_node) = self.arena[old_h].as_mut() {
                old_node.prev = Some(idx);
            }
        }

        // Crown the new node as head
        self.head = Some(idx);

        // If the list was empty, this node is also the tail
        if self.tail.is_none() {
            self.tail = Some(idx);
        }
    }

    // ── DLL Operation: Unlink ────────────────────────────────────
    // Removes a node from its current position WITHOUT freeing it.
    // This is used before re-linking (on_access) or before freeing (evict/delete).
    //
    //   Before:  ... ←→ [prev] ←→ [THIS] ←→ [next] ←→ ...
    //   After:   ... ←→ [prev] ←→ [next] ←→ ...
    //            [THIS] is now floating (prev/next not cleared)
    //
    fn unlink(&mut self, idx: usize) {
        let (prev, next) = {
            let node = self.arena[idx].as_ref().unwrap();
            (node.prev, node.next)
        };

        // Stitch the predecessor to the successor (or update head)
        match prev {
            Some(p) => self.arena[p].as_mut().unwrap().next = next,
            None => self.head = next, // This node WAS the head
        }

        // Stitch the successor to the predecessor (or update tail)
        match next {
            Some(n) => self.arena[n].as_mut().unwrap().prev = prev,
            None => self.tail = prev, // This node WAS the tail
        }
    }
}

impl<K: Eq + Hash + Clone> EvictionPolicy<K> for LruPolicy<K> {
    fn on_insert(&mut self, key: K) {
        // 1. Allocate a slot in the arena
        let idx = self.alloc();

        // 2. Create the node and place it in the arena
        self.arena[idx] = Some(LruNode {
            key: key.clone(),
            prev: None,
            next: None,
        });

        // 3. Link it at the head (most recently used position)
        self.link_at_head(idx);

        // 4. Register the key → index mapping for O(1) lookup
        self.key_to_index.insert(key, idx);
    }

    fn on_access(&mut self, key: &K) {
        if let Some(&idx) = self.key_to_index.get(key) {
            // Yank the node out of its current position...
            self.unlink(idx);
            // ...and slam it back at the head (most recently used).
            // This is the core LRU operation: O(1) because we have
            // the index and DLL operations are pointer (index) swaps.
            self.link_at_head(idx);
        }
    }

    fn evict(&mut self) -> Option<K> {
        // The tail is always the Least Recently Used node.
        let tail_idx = self.tail?;

        // 1. Unlink the tail from the list
        self.unlink(tail_idx);

        // 2. Take the node out of the arena (the slot becomes None)
        let node = self.arena[tail_idx].take().unwrap();

        // 3. Remove the key from the lookup map
        self.key_to_index.remove(&node.key);

        // 4. Recycle the arena slot for future allocations
        self.free_list.push(tail_idx);

        Some(node.key)
    }

    fn on_delete(&mut self, key: &K) {
        if let Some(idx) = self.key_to_index.remove(key) {
            // 1. Unlink from the DLL
            self.unlink(idx);

            // 2. Free the arena slot
            self.arena[idx] = None;
            self.free_list.push(idx);
        }
    }
}

// ==========================================
// LFU POLICY (Arena-Based Frequency Buckets)
// ==========================================
//
// THE O(1) LFU DESIGN:
// The classic O(1) LFU uses a HashMap of frequency → doubly linked list.
// Each frequency "bucket" holds all keys with that access count.
// Within a bucket, nodes are ordered by insertion time (FIFO tiebreaking).
//
// Architecture:
//   ┌─────────┐     ┌─────────────────────────────────────┐
//   │ min_freq │────►│ bucket[1]: tail ←→ ... ←→ head     │ ← eviction target
//   └─────────┘     │ bucket[2]: tail ←→ ... ←→ head     │
//                   │ bucket[5]: tail ←→ ... ←→ head     │
//                   └─────────────────────────────────────┘
//
// On access, a key moves from bucket[freq] to bucket[freq+1].
// On eviction, we pop the TAIL of bucket[min_freq] (oldest among
// the least frequently used keys).
//
// We reuse the same Arena pattern from LRU — all nodes live in a
// shared Vec, and each frequency bucket just stores head/tail indices.
//
// COMPLEXITY: O(1) for insert, access, evict, and delete.

/// A single node in an LFU frequency bucket's doubly linked list.
struct LfuNode<K> {
    key: K,
    freq: usize,          // Current access frequency of this key
    prev: Option<usize>,  // Previous node in THIS frequency bucket's DLL
    next: Option<usize>,  // Next node in THIS frequency bucket's DLL
}

/// A frequency bucket: the head/tail of a DLL containing all keys
/// with a specific access frequency.
struct FreqBucket {
    head: Option<usize>, // Most recently added to this frequency
    tail: Option<usize>, // Oldest in this frequency (eviction target)
}

pub struct LfuPolicy<K> {
    // Shared arena: ALL nodes across ALL frequency buckets live here.
    arena: Vec<Option<LfuNode<K>>>,

    // O(1) lookup: key → arena index
    key_to_index: HashMap<K, usize>,

    // Each distinct frequency has its own DLL (tracked by head/tail indices).
    buckets: HashMap<usize, FreqBucket>,

    // Tracks the current minimum frequency across all keys.
    // This lets us find the eviction target in O(1).
    min_freq: usize,

    // Recycled arena slots.
    free_list: Vec<usize>,
}

impl<K: Eq + Hash + Clone> LfuPolicy<K> {
    pub fn new() -> Self {
        Self {
            arena: Vec::new(),
            key_to_index: HashMap::new(),
            buckets: HashMap::new(),
            min_freq: 0,
            free_list: Vec::new(),
        }
    }

    fn alloc(&mut self) -> usize {
        if let Some(idx) = self.free_list.pop() {
            idx
        } else {
            let idx = self.arena.len();
            self.arena.push(None);
            idx
        }
    }

    // ── DLL Operation: Link at Head of a Frequency Bucket ────────
    // Inserts a node at the HEAD of a specific frequency's DLL.
    // If the bucket doesn't exist yet, it is created.
    fn link_at_head_of_bucket(&mut self, freq: usize, idx: usize) {
        // Step 1: Get or create the bucket, extract the old head.
        // We scope this block so the mutable borrow of `self.buckets`
        // is released before we touch `self.arena`.
        let old_head = {
            let bucket = self.buckets.entry(freq).or_insert(FreqBucket {
                head: None,
                tail: None,
            });
            let old_head = bucket.head;
            bucket.head = Some(idx);
            if bucket.tail.is_none() {
                bucket.tail = Some(idx);
            }
            old_head
        }; // ← borrow of self.buckets released here

        // Step 2: Wire up the node's pointers
        if let Some(node) = self.arena[idx].as_mut() {
            node.prev = None;
            node.next = old_head;
        }

        // Step 3: Tell the old head about its new predecessor
        if let Some(old_h) = old_head {
            if let Some(old_node) = self.arena[old_h].as_mut() {
                old_node.prev = Some(idx);
            }
        }
    }

    // ── DLL Operation: Unlink from a Frequency Bucket ────────────
    // Removes a node from a specific frequency's DLL without freeing it.
    fn unlink_from_bucket(&mut self, freq: usize, idx: usize) {
        // Step 1: Read the node's neighbors
        let (prev, next) = {
            let node = self.arena[idx].as_ref().unwrap();
            (node.prev, node.next)
        };

        // Step 2: Stitch neighbors together in the arena
        if let Some(p) = prev {
            self.arena[p].as_mut().unwrap().next = next;
        }
        if let Some(n) = next {
            self.arena[n].as_mut().unwrap().prev = prev;
        }

        // Step 3: Update the bucket's head/tail if this node was at an edge
        if let Some(bucket) = self.buckets.get_mut(&freq) {
            if bucket.head == Some(idx) {
                bucket.head = next;
            }
            if bucket.tail == Some(idx) {
                bucket.tail = prev;
            }
        }
    }

    // ── Helper: Remove empty bucket and return whether it was removed ──
    fn cleanup_bucket(&mut self, freq: usize) -> bool {
        if let Some(bucket) = self.buckets.get(&freq) {
            if bucket.head.is_none() {
                self.buckets.remove(&freq);
                return true;
            }
        }
        false
    }
}

impl<K: Eq + Hash + Clone> EvictionPolicy<K> for LfuPolicy<K> {
    fn on_insert(&mut self, key: K) {
        // Every new key starts with frequency 1.
        let idx = self.alloc();
        self.arena[idx] = Some(LfuNode {
            key: key.clone(),
            freq: 1,
            prev: None,
            next: None,
        });

        // Add to the frequency-1 bucket
        self.link_at_head_of_bucket(1, idx);
        self.key_to_index.insert(key, idx);

        // A brand-new key ALWAYS has the lowest possible frequency.
        self.min_freq = 1;
    }

    fn on_access(&mut self, key: &K) {
        if let Some(&idx) = self.key_to_index.get(key) {
            // Read the current frequency (usize is Copy, no borrow held)
            let old_freq = self.arena[idx].as_ref().unwrap().freq;
            let new_freq = old_freq + 1;

            // 1. Yank the node out of its current frequency bucket
            self.unlink_from_bucket(old_freq, idx);

            // 2. If the old bucket is now empty, remove it.
            //    If it was the min_freq bucket, the new min is old_freq + 1.
            //    This is GUARANTEED correct because we just moved the last
            //    node from min_freq to min_freq + 1.
            if self.cleanup_bucket(old_freq) && old_freq == self.min_freq {
                self.min_freq = new_freq;
            }

            // 3. Promote: update the node's frequency and link to the new bucket
            self.arena[idx].as_mut().unwrap().freq = new_freq;
            self.link_at_head_of_bucket(new_freq, idx);
        }
    }

    fn evict(&mut self) -> Option<K> {
        // Find the TAIL of the min_freq bucket — that's the least frequently
        // used key (with FIFO tiebreaking: oldest among equal frequencies).
        let tail_idx = {
            let bucket = self.buckets.get(&self.min_freq)?;
            bucket.tail?
        }; // ← borrow released before mutation

        // 1. Unlink the victim from its frequency bucket
        self.unlink_from_bucket(self.min_freq, tail_idx);

        // 2. Extract the node from the arena
        let node = self.arena[tail_idx].take().unwrap();
        self.key_to_index.remove(&node.key);
        self.free_list.push(tail_idx);

        // 3. Clean up empty bucket.
        //    If it was the min_freq bucket, scan for the new minimum.
        if self.cleanup_bucket(self.min_freq) {
            self.min_freq = self.buckets.keys().copied().min().unwrap_or(0);
        }

        Some(node.key)
    }

    fn on_delete(&mut self, key: &K) {
        if let Some(idx) = self.key_to_index.remove(key) {
            let freq = self.arena[idx].as_ref().unwrap().freq;

            // 1. Unlink from its frequency bucket
            self.unlink_from_bucket(freq, idx);

            // 2. Free the arena slot
            self.arena[idx] = None;
            self.free_list.push(idx);

            // 3. If the bucket is now empty, remove it.
            //    If it was the min_freq bucket, scan for the new minimum.
            //    (Delete is rare, so an O(F) scan is acceptable here.)
            if self.cleanup_bucket(freq) && freq == self.min_freq {
                self.min_freq = self.buckets.keys().copied().min().unwrap_or(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================
    // LRU ARENA TESTS
    // ==========================================

    #[test]
    fn test_lru_arena_recycling() {
        let mut policy = LruPolicy::new();

        policy.on_insert("A"); // idx 0
        policy.on_insert("B"); // idx 1
        assert_eq!(policy.arena.len(), 2);
        assert_eq!(policy.free_list.len(), 0);

        // Delete "A", releasing idx 0
        policy.on_delete(&"A");
        assert_eq!(policy.free_list, vec![0]);
        assert!(policy.arena[0].is_none());

        // Insert "C", which should recycle idx 0 instead of expanding the arena
        policy.on_insert("C"); // should take idx 0
        assert_eq!(policy.arena.len(), 2, "Arena should not have grown");
        assert_eq!(policy.free_list.len(), 0);
        assert_eq!(policy.arena[0].as_ref().unwrap().key, "C");
    }

    #[test]
    fn test_lru_head_tail_updates() {
        let mut policy = LruPolicy::new();
        
        policy.on_insert("A");
        assert_eq!(policy.head, Some(0));
        assert_eq!(policy.tail, Some(0)); // Only one element, so head == tail

        policy.on_insert("B");
        assert_eq!(policy.head, Some(1)); // B is MRU
        assert_eq!(policy.tail, Some(0)); // A is LRU

        // Access A, making it MRU
        policy.on_access(&"A");
        assert_eq!(policy.head, Some(0)); // A is now MRU
        assert_eq!(policy.tail, Some(1)); // B is now LRU
    }

    #[test]
    fn test_lru_eviction() {
        let mut policy = LruPolicy::new();
        
        policy.on_insert("A");
        policy.on_insert("B");
        policy.on_insert("C");

        // Order is MRU [C, B, A] LRU
        assert_eq!(policy.evict(), Some("A")); // A was inserted first and never accessed
        
        // Order is now MRU [C, B] LRU
        policy.on_access(&"B"); // Order becomes MRU [B, C] LRU
        
        assert_eq!(policy.evict(), Some("C"));
        assert_eq!(policy.evict(), Some("B"));
        assert_eq!(policy.evict(), None); // Empty
    }

    #[test]
    fn test_lru_single_element_eviction_and_delete() {
        let mut policy = LruPolicy::new();
        
        policy.on_insert("A");
        policy.on_delete(&"A");
        assert_eq!(policy.head, None);
        assert_eq!(policy.tail, None);
        assert_eq!(policy.evict(), None);

        policy.on_insert("B");
        assert_eq!(policy.evict(), Some("B"));
        assert_eq!(policy.head, None);
        assert_eq!(policy.tail, None);
    }

    // ==========================================
    // LFU ARENA TESTS
    // ==========================================

    #[test]
    fn test_lfu_basic_eviction() {
        let mut policy = LfuPolicy::new();
        
        policy.on_insert("A"); // freq 1
        policy.on_insert("B"); // freq 1
        policy.on_insert("C"); // freq 1

        policy.on_access(&"A"); // freq 2
        policy.on_access(&"B"); // freq 2

        // C has freq 1. A and B have freq 2.
        assert_eq!(policy.evict(), Some("C"));

        // A and B both have freq 2. A was inserted before B, so A is the "oldest" among ties.
        // Wait, FIFO tiebreaker means the one that entered the current frequency bucket first is at the TAIL.
        // `A` entered freq 2 before `B`. So `A` is older in the freq 2 bucket.
        assert_eq!(policy.evict(), Some("A"));
        assert_eq!(policy.evict(), Some("B"));
    }

    #[test]
    fn test_lfu_min_freq_tracking() {
        let mut policy = LfuPolicy::new();
        
        policy.on_insert("A"); // min_freq = 1
        assert_eq!(policy.min_freq, 1);
        
        policy.on_access(&"A"); // freq = 2. min_freq should become 2 because bucket 1 is empty.
        assert_eq!(policy.min_freq, 2);

        policy.on_insert("B"); // min_freq = 1 again
        assert_eq!(policy.min_freq, 1);

        policy.on_delete(&"B"); // min_freq should jump back to 2 (the only remaining key is A)
        assert_eq!(policy.min_freq, 2);
    }

    #[test]
    fn test_lfu_arena_recycling() {
        let mut policy = LfuPolicy::new();

        policy.on_insert("A"); // idx 0
        policy.on_insert("B"); // idx 1
        assert_eq!(policy.arena.len(), 2);
        
        policy.on_delete(&"A"); // frees idx 0
        assert_eq!(policy.free_list, vec![0]);
        
        policy.on_insert("C"); // reuses idx 0
        assert_eq!(policy.arena.len(), 2);
        assert_eq!(policy.arena[0].as_ref().unwrap().key, "C");
        assert_eq!(policy.free_list.len(), 0);
    }

    #[test]
    fn test_lfu_cleanup_bucket_on_delete() {
        let mut policy = LfuPolicy::new();
        
        policy.on_insert("A");
        policy.on_access(&"A"); // freq = 2
        
        assert_eq!(policy.buckets.contains_key(&1), false, "Bucket 1 should have been cleaned up");
        assert_eq!(policy.buckets.contains_key(&2), true, "Bucket 2 should exist");

        policy.on_delete(&"A");
        assert_eq!(policy.buckets.contains_key(&2), false, "Bucket 2 should be cleaned up after delete");
        assert_eq!(policy.min_freq, 0, "No keys left, min_freq should be 0");
    }
}