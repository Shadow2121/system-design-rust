# Internal Architecture & Implementation Reference

## 1. Design Philosophy
This module was built to serve as the local storage engine for a distributed system, specifically targeting rate-limiting capabilities for automated agents. The primary architectural decision was to decouple **data mapping** from **memory management** using the Strategy Pattern (`EvictionPolicy` trait), and to separate the **storage logic** from the **concurrency model**.

## 2. Phase 1: The Core Engine & Eviction
The core engine (`KvStore`) handles data insertion and capacity enforcement. It relies on injected policies to determine memory victims.

* **FIFO (`FifoPolicy`)**: Uses a `VecDeque`. O(1) tracking.
* **LRU (`LruPolicy`)**: Uses a `VecDeque`. O(N) access updates. 
* **LFU (`LfuPolicy`)**: Uses a `HashMap<String, usize>`. O(N) evictions.
* **Lazy TTL**: Expiration timestamps are checked upon `get()`. If the current time exceeds the expiry, the key is immediately dropped. This prevents the need for a CPU-intensive background garbage collection thread.

## 3. Phase 2: The Concurrent Core
To survive a multi-threaded web server environment, the single-threaded `KvStore` was wrapped in a `ConcurrentKvStore` struct. 

### The `Arc<RwLock<T>>` Pattern
* **`RwLock` (Access Control):** We chose a Read-Write Lock over a standard `Mutex`. In a rate limiter, you might have many services checking a token count simultaneously without updating it. `RwLock` allows infinite concurrent readers, but strictly sequential writers, maximizing throughput.
* **`Arc` (Memory Lifetimes):** The lock is wrapped in an Atomic Reference Counted pointer. When a new thread is spawned, we clone the `Arc` (incrementing a counter) rather than copying the HashMap. Once all threads finish, the count hits zero, and Rust safely drops the database from the heap.

### Implementation Nuance: The Trait Bound Macro Bug
During Phase 2, we encountered a strict constraint with Rust's `#[derive(Clone)]` macro. When applied to `ConcurrentKvStore<P>`, the macro demanded that the generic policy `P` also implement `Clone`. 

* **The Fix:** We discarded the macro and wrote a manual implementation of the `Clone` trait for `ConcurrentKvStore`. By doing this, we explicitly instructed the compiler to only clone the `Arc` pointer. This bypasses the generic bounds, allowing us to use non-cloneable policies safely within the locked pointer.

### Implementation Nuance: Interior Mutability
Because the `RwLock` handles state changes internally, the `ConcurrentKvStore` methods (`set`, `get`, `delete`) only require an immutable reference (`&self`). Note that our `get` method actually requests a `.write()` lock instead of a `.read()` lock. This is mandatory because reading a key triggers the `policy.on_access()` hook, which mutates the internal tracking queues (e.g., updating an LRU position).

## 4. Known Limitations (Transitioning to Phase 3)
The engine is currently thread-safe for OS-level threads (`std::thread`), but it is not yet exposed to the network. 

To act as a standalone service within the workspace, this crate must be wrapped in an asynchronous runtime (like `tokio`) and an HTTP framework (like `axum`). Care must be taken when mixing standard library `RwLock` blocking calls with `tokio`'s async worker pools to avoid starving the async reactor.