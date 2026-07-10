# Internal Architecture & Implementation Reference

## 1. Design Philosophy
This module was built to serve as the local storage engine for a distributed system, specifically targeting rate-limiting capabilities for automated agents. The primary architectural decision was to decouple **data mapping** from **memory management** using the Strategy Pattern (`EvictionPolicy` trait), and to separate the **storage logic** from the **concurrency model**.

## 2. Generic Type System
The entire crate is generic over key type `K` and value type `V`:
* **Key bounds:** `K: Eq + Hash + Clone` — required for HashMap lookup and policy tracking.
* **Value bounds:** `V: Clone` — required because `get()` returns a copy.
* **Thread-safety bounds:** `K: Send + Sync + 'static`, `V: Send + Sync + 'static` — required only for `ConcurrentKvStore`.

This allows the store to hold any key-value pair (e.g., `(IpAddr, RateLimitEntry)`, `(UserId, SessionData)`) without code changes. Future network layers can add `serde::Serialize + Deserialize` via a Cargo feature flag.

## 3. Phase 1: The Core Engine & Eviction
The core engine (`KvStore`) handles data insertion and capacity enforcement. It relies on injected policies to determine memory victims.

### Eviction Policies

* **FIFO (`FifoPolicy`)**: Uses a `VecDeque`. O(1) insert, O(1) evict, O(N) delete.
* **LRU (`LruPolicy`)**: Uses a `HashMap<K, u64>` + `BTreeMap<u64, K>` dual-map with a monotonic counter as a logical clock. The counter tracks recency — higher values mean more recent access. Eviction pops the smallest counter from the BTreeMap. **O(log N) for all operations.**
* **LFU (`LfuPolicy`)**: Uses a `HashMap<K, (usize, u64)>` + `BTreeMap<(usize, u64), K>` dual-map. The BTreeMap key is `(frequency, order_id)`, which sorts lexicographically — lowest frequency first, with FIFO tiebreaking among equal frequencies. **O(log N) for all operations.**

### Ghost Key Prevention
All policies implement `on_delete(key)`, called by `KvStore::delete()` and by the lazy TTL expiry path in `get()`. This ensures the policy's tracking structures stay synchronized with the actual data, preventing phantom eviction targets.

### Lazy TTL
Expiration timestamps are checked upon `get()`. If the current time exceeds the expiry, the key is immediately dropped from both the HashMap and the policy. This prevents the need for a CPU-intensive background garbage collection thread.

## 4. Phase 2: The Concurrent Core (Split-Lock Architecture)

### The Problem with Single-Lock
The original design wrapped the entire `KvStore` in a single `Arc<RwLock<KvStore>>`. Because `get()` mutates policy state (`on_access`), it required a `.write()` lock — making ALL operations (including reads) serialize. The `RwLock` was effectively behaving as a `Mutex`.

### The Solution: Data Plane / Control Plane Separation
`ConcurrentKvStore` now holds two independent locks:

```
Data Plane:    Arc<RwLock<HashMap<K, CacheEntry<V>>>>  → many concurrent readers
Control Plane: Arc<Mutex<P>>                            → serialized policy updates
```

#### `get()` — The Big Win
The read path uses two small, separate critical sections:
1. **`data.read()`** — shared read lock, fully concurrent with other readers
2. **`policy.lock()`** — brief exclusive lock for access tracking only

Multiple threads reading different (or the same) keys no longer block each other during the data read phase. They only briefly serialize during the policy bookkeeping.

#### `set()` and `delete()` — Atomic Writes
Write operations hold both locks simultaneously for atomicity between eviction decisions and data modifications. Lock ordering is always **policy → data** to prevent deadlocks.

### Implementation Nuance: The Trait Bound Macro Bug
During the original implementation, we encountered a strict constraint with Rust's `#[derive(Clone)]` macro. When applied to `ConcurrentKvStore<K, V, P>`, the macro demanded that the generic policy `P` also implement `Clone`.

* **The Fix:** We discarded the macro and wrote a manual implementation of the `Clone` trait. By doing this, we explicitly instructed the compiler to only clone the `Arc` pointers. This bypasses the generic bounds, allowing us to use non-cloneable policies safely within the locked pointers.

## 5. Industry Context: Where We Sit

| System | Concurrency Model | Policy Accuracy | Our Analogue |
|--------|-------------------|-----------------|-------------|
| **Redis** | Single-threaded command loop | Perfect | N/A — different philosophy |
| **Caffeine/Moka** | Lock-free HashMap + lossy ring buffers | Eventually consistent | Our split-lock is a simpler version of this |
| **Memcached** | Per-slab-class locks + sub-LRU sharding | Perfect per-shard | Our data RwLock is analogous to per-slab locks |
| **Our design** | RwLock (data) + Mutex (policy) | Perfect | — |

The next evolution beyond our split-lock would be the Caffeine "buffer and batch" pattern: log access events into per-thread ring buffers, drain them periodically into the policy. This trades perfect accuracy for lock-free reads.

## 6. Known Limitations (Transitioning to Phase 3)
The engine is currently thread-safe for OS-level threads (`std::thread`), but it is not yet exposed to the network.

To act as a standalone service within the workspace, this crate must be wrapped in an asynchronous runtime (like `tokio`) and an HTTP framework (like `axum`). Care must be taken when mixing standard library `RwLock`/`Mutex` blocking calls with `tokio`'s async worker pools to avoid starving the async reactor. The recommended approach is to use `tokio::sync::RwLock` and `tokio::sync::Mutex` instead.