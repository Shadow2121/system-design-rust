# Arena Allocator (Index-as-Pointer)

## The Problem
In Rust, implementing a Doubly Linked List (DLL) where you need to access arbitrary nodes in O(1) time is extremely challenging. The borrow checker forbids multiple pointers from mutating the same data (shared mutable state), which is exactly what `prev` and `next` pointers do in a traditional DLL. This makes O(1) LRU or LFU cache policies difficult to implement safely.

## The Solution: Arena Algorithm
Instead of using memory pointers (which the borrow checker restricts), we use an **Arena** (a simple `Vec`) and replace pointers with array indices (`usize`). 

1. **Memory Pre-allocation**: We put all nodes into one large array (`Vec`).
2. **Index as Pointers**: Each node stores the index of its `prev` and `next` siblings in the array, rather than actual memory addresses.
3. **Safe Updates**: Because we are just updating integer numbers instead of memory addresses, we achieve the exact same O(1) lightning-fast performance as C++, but remain 100% memory-safe under Rust's strict borrow checker rules.

### Use in LRU / LFU
- **LRU (Least Recently Used)**: We maintain `head` and `tail` indices. When a key is accessed, we update its `prev` and `next` indices to move it to the `head` of the array logically, without moving it in memory.
- **LFU (Least Frequently Used)**: We use a "List of Lists" pattern. A `HashMap` maps frequencies (e.g., "1 access") to a bucket tracking its own `head` and `tail`. When a node's frequency increases, it doesn't move in memory; we simply rewrite its `prev`/`next` numbers to unlink it from the "Frequency 1" list and link it to the "Frequency 2" list.

This approach is highly performant and used in production-grade Rust caches.
