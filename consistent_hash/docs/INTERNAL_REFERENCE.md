# Architecture Decisions: `consistent_hash`

## Overview
This crate provides a pure Rust, synchronous implementation of a Hash Ring, fulfilling the requirements for distributed key-value storage routing. It is designed with performance and correctness in mind, utilizing virtual nodes to ensure even data distribution.

## Key Design Choices

### 1. Data Structure: `Vec<(u64, N)>` vs `BTreeMap`
The hash ring is implemented as a sorted `Vec<(u64, N)>` where `u64` is the hash and `N` is the node identifier.
- **Why not `BTreeMap`?** While `BTreeMap` provides O(log N) inserts and lookups, iterating to find the "next" element or wrapping around is less ergonomic and often has more memory overhead per node than a flat vector. 
- **Binary Search**: Since the ring represents read-heavy data (lookups are vastly more common than node additions/removals), a sorted vector with binary search (`binary_search_by_key`) provides optimal cache locality and O(log N) lookups. Node additions take O(V * N) where V is `vnodes_per_node`, which is acceptable since topology changes are rare.

### 2. Hashing Algorithm: `std::hash::DefaultHasher`
- We use the standard library's `DefaultHasher` (SipHash 1-3).
- **Reasoning**: It is fast, cryptographically resistant to hash collision attacks, and requires no external dependencies, satisfying the strict "pure Rust, no dependencies" requirement for this crate. While its internal random state differs between process executions (unless specifically seeded), for the purposes of consistent hashing *within a running cluster process* (or within integration tests), it provides robust deterministic routing.

### 3. Virtual Nodes (vnodes)
- Each physical node is represented by multiple virtual nodes (defaulting to ~150).
- This drastically improves the evenness of key distribution compared to a naive 1-to-1 hash ring, where a single node might accidentally become responsible for a disproportionately large slice of the hash space.
- Virtual node hashes are computed by hashing the node identifier alongside a running index `(node, vnode_idx)`.

### 4. Replica Set Collection
- The `get_replicas(key, n)` method correctly handles virtual nodes by ensuring it only returns *distinct* physical nodes. It walks the ring from the key's hash position and adds nodes to a set until `n` distinct physical nodes are found or the ring is exhausted.
