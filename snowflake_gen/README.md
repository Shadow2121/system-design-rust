# Distributed ID Generator (`snowflake_gen`)

**Author:** Mihir Patel  
**Language:** Rust  
**Architecture:** Twitter Snowflake (Modified)  

## 1. System Overview

This crate provides a highly concurrent, lock-free distributed ID generator designed for large-scale system architectures. It generates strictly unique, 64-bit, time-sortable integers. It is designed to act as the core primary key generation mechanism for high-throughput write paths, eliminating the bottlenecks associated with centralized database `AUTO_INCREMENT` or `SERIAL` locks.

This project is structured as a library crate within a larger Cargo Workspace (`system-design-rust`), allowing other modules (e.g., Rate Limiters, Load Balancers, URL Shorteners) to import it as a local dependency.

---

## 2. The Snowflake Algorithm & Bit Layout

The core generator constructs a 64-bit integer using bitwise operations (`<<` and `|`). The integer is divided into four structural components:

* **Sign Bit (1 bit):** Always `0` to ensure the resulting ID is a positive integer.
* **Timestamp (41 bits):** Milliseconds elapsed since a custom epoch (`1_704_067_200_000` -> Jan 1, 2024). This provides ~69 years of capacity and guarantees the IDs are roughly time-sortable.
* **Machine ID (10 bits):** The unique identifier for the worker node generating the ID. Max capacity: 1,024 nodes.
* **Sequence (12 bits):** A local counter initialized to `0` every new millisecond. Supports generating up to 4,096 unique IDs per millisecond, per node.

### Bitwise Assembly Logic
```rust
let id = (current_time << 22) | (machine_id << 12) | sequence;
```
## 3. Core Implementation Details (`src/lib.rs`)

### Concurrency & State Management
* **State Struct:** The mutable state consists of `last_timestamp` and `sequence`.
* **Thread Safety:** The state is wrapped in a `std::sync::Mutex`. Due to Rust's RAII (Resource Acquisition Is Initialization), the lock is automatically acquired at the start of the `generate_id()` method and dropped implicitly at the end of the scope.
* **Performance:** Because the critical section only performs basic arithmetic and bitwise shifts, lock contention is virtually zero, allowing a single node to easily exceed 1,000+ IDs/sec.
* **Sequence Overflow:** If the 12-bit sequence hits its maximum (4095) within a single millisecond, the thread enters a CPU spin-wait loop until the system clock ticks to the next millisecond.

### NTP Clock Drift Safeguards
The system employs a hybrid heuristic to handle backward clock ticks caused by Network Time Protocol (NTP) adjustments:
1.  **Micro-Drift Tolerance (`<= 5ms`):** If the clock ticks backward by 5ms or less, the thread sleeps for the drift duration. This absorbs standard jitter silently without failing upstream requests.
2.  **Macro-Drift Protection (`> 5ms`):** If the clock shifts significantly, the system fails fast, returning a `ClockMovedBackwardsError`. This prevents thread-pool exhaustion and allows load balancers to route traffic away from the degraded node.

---

## 4. Architectural Sequence Experiments (`src/main.rs`)

Beyond the core library, this project includes architectural simulations demonstrating how to handle business requirements for **strict, gapless sequence IDs** (e.g., `INV-1001`). 

### Architecture A: Hybrid Model (Separation of Concerns)
* **Concept:** Keeps the critical API path fast while handling gapless generation in the background.
* **Implementation:** Utilizes Rust's native `std::sync::mpsc` (Multiple Producer, Single Consumer) channels.
* **Fast Path:** Concurrent web threads generate Snowflake IDs instantly for internal DB primary keys.
* **Slow Path:** The IDs are pushed to the channel. A single-threaded background worker pulls from the channel and sequentially assigns gapless business IDs, avoiding complex distributed locks.

### Architecture B: Distributed Lock Manager (Anti-Pattern Demo)
* **Concept:** Forces strict sequential generation directly on the highly concurrent API write path.
* **Implementation:** Simulates network latency and a global consensus lock (mimicking `etcd` or Redis Redlock).
* **Result:** Demonstrates severe throughput degradation. Threads are forced to wait on network I/O to acquire and release the lock, effectively reducing the highly concurrent API to a single-threaded bottleneck.

---

## 5. Usage & Integration

To use this generator in another crate within the workspace, add the following to the consumer's `Cargo.toml`:

```toml
[dependencies]
snowflake_gen = { path = "../snowflake_gen" }
```
**Initialization:**
```rust
use snowflake_gen::SnowflakeGenerator;
use std::sync::Arc;

// Initialize with Machine ID 1
let generator = Arc::new(SnowflakeGenerator::new(1).unwrap());

// Generate an ID
let internal_id = generator.generate_id().unwrap();
```