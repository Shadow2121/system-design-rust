# kv_store

## ⚡ Current State
The `kv_store` crate is a fully functional, thread-safe, in-memory cache library with O(1) eviction policies (FIFO, LRU, LFU) and lazy TTL. It is currently a single-node embedded cache (analogous to Moka or Caffeine). 
Recent design improvements identified:
- **Split-lock architecture**: Migrating from a single `RwLock` wrapping the entire store to separate locks: `RwLock` for the data HashMap and `Mutex` for the policy. This eliminates the "write-lock on get" bottleneck and enables true read concurrency.
- **Generic types**: Refactoring from hardcoded `String, String` to generic `<K, V>` types with appropriate trait bounds (`Eq + Hash + Clone + Send + Sync`).
Industry context: Production caches use lock-free HashMaps + ring buffers (like Window TinyLFU in Caffeine) to amortize lock contention. Our split-lock is a simpler, highly effective analogue.

## 📖 History
### Update from transcript 71d13dc2-680e-4e7d-8ac0-f02853124b26
- Identified read concurrency bottleneck in `ConcurrentKvStore.get()` due to `.write()` lock requirements for policy updates.
- Decided to adopt a split-lock architecture to decouple the data plane (`RwLock`) and control plane (`Mutex`).
- Decided to refactor hardcoded `String` types to generics `<K, V>`.

### Update from transcript 5914962c-1bb8-4e24-801f-8e84349117b1
- Audited the current implementation and classified it as an embedded cache engine, not a full KV store yet. Practical for rate limiting or memoization.
