# consistent_hash

## ⚡ Current State
The `consistent_hash` crate is **complete**. It implements a hash ring with virtual nodes for consistent key-to-node mapping, used for distributed key routing.

Key details:
- **Data Structure**: Sorted `Vec<(u64, N)>` with binary search — O(log N) lookups, excellent cache locality.
- **Generic**: `HashRing<N>` where `N: Clone + Hash + Eq`. Works with `&str`, `String`, or custom node IDs.
- **Hashing**: `std::hash::DefaultHasher` (SipHash 1-3). Zero external dependencies.
- **Virtual Nodes**: ~150 vnodes per physical node by default for even distribution.
- **API**: `add_node`, `remove_node`, `get_node(key)`, `get_replicas(key, n)`.
- **Replica Collection**: Walks the ring collecting distinct physical nodes, correctly skipping duplicate vnodes.
- **No unsafe code**.

## 📖 History
### Update from transcript e2255da9-fd7e-449a-99ef-9eb7765ed471
- Planned and implemented the `consistent_hash` crate as Crate 3 in the workspace roadmap.
- Chose `Vec<(u64, N)>` over `BTreeMap` for cache locality on the read-heavy ring.
- Chose `DefaultHasher` to satisfy "no external dependencies" constraint.
- Set default vnodes to 150 per physical node.
- Created 6 integration tests proving determinism, even distribution, minimal key movement, replica uniqueness, graceful degradation, and empty ring safety.
- Added `INTERNAL_REFERENCE.md` documenting all architecture decisions.
