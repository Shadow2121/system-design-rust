# High-Performance Key-Value Store (`kv_store`)

A modular, highly configurable, thread-safe in-memory key-value store implemented in Rust. Designed specifically as the foundational storage layer for high-throughput distributed systems, such as API rate limiters and real-time AI agent platforms.

## Features
* **Fearless Concurrency:** Built with an `Arc<RwLock>` architecture, allowing thousands of concurrent reads and thread-safe exclusive writes without data races.
* **Pluggable Eviction Strategies:** Inject `FifoPolicy`, `LruPolicy`, or `LfuPolicy` dynamically via Rust traits depending on the downstream workload.
* **Hard Capacity Limits:** Enforces a strict maximum key count to guarantee memory safety and prevent Out-Of-Memory (OOM) crashes under heavy load.
* **Lazy TTL Expiration:** Supports per-key Time-To-Live (TTL) without the computational overhead of background sweeping threads.
* **Zero-Cost Abstractions:** Built utilizing standard Rust collections to minimize dependency bloat while maintaining O(1) read/write performance.

## Installation
This crate is designed to be used within a Cargo workspace. Add it to your downstream application's `Cargo.toml`:

```toml
[dependencies]
kv_store = { path = "../kv_store" }
```

## Quick Start & Usage

Instantiate the concurrent store by defining a capacity limit and injecting your desired eviction strategy. The store can be safely shared across multiple threads.

```rust
use kv_store::{ConcurrentKvStore, LruPolicy};
use std::time::Duration;
use std::thread;

fn main() {
    // Initialize a thread-safe store capable of holding 100,000 keys
    let store = ConcurrentKvStore::new(100_000, LruPolicy::new());

    // Clone the pointer (not the database) to share with a background worker
    let worker_store = store.clone();

    thread::spawn(move || {
        // Safely write to the store from a different thread
        worker_store.set(
            "user_ip_127_0_0_1".to_string(), 
            "token_count: 5".to_string(), 
            Some(Duration::from_secs(60))
        );
    }).join().unwrap();

    // Safely read the data from the main thread
    if let Some(value) = store.get("user_ip_127_0_0_1") {
        println!("Data: {}", value);
    }
}
```

## Architecture
This crate separates the underlying storage engine (`store.rs`) from the eviction logic (`policy.rs`) using the `EvictionPolicy` trait. The core engine is then wrapped in a `ConcurrentKvStore` which manages memory synchronization (`Arc`) and access control (`RwLock`), allowing downstream services to dictate their own memory management rules safely in highly concurrent environments.