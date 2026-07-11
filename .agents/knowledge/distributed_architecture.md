# Distributed Architecture

## ⚡ Current State
The project aims to build a fully distributed DynamoDB-style KV store. 
Roadmap:
1. **Network Layer (Phase 3)**: Expose `kv_store` via async `tokio` + `axum` HTTP server. Crucially, switch from `std::sync` to `tokio::sync` locks to prevent reactor starvation.
2. **Distribution (Phase 5)**: Consistent hashing, smart client routing, gossip protocol for membership, hinted handoff, and read repair.
Local testing model: A node is just a process on a distinct port (e.g., 7001, 7002, 7003). Partitioning, replication, and node failures can all be simulated by running multiple processes and killing them locally.

## 📖 History
### Update from transcript 5914962c-1bb8-4e24-801f-8e84349117b1
- Defined the roadmap to a true distributed system (network, persistence, sharding, observability).
- Outlined how to test distributed systems locally using multiple ports and processes.
- Detailed the DynamoDB architecture (smart routing, gossip, hinted handoff) and decided to implement it as a crate.
