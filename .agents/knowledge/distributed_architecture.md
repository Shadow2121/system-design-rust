# Distributed Architecture

## ⚡ Current State
The project aims to build a fully distributed DynamoDB-style KV store.
Progress:
- ✅ `kv_store` — Complete (sync cache engine with O(1) eviction)
- ✅ `snowflake_gen` — Complete (distributed ID generation)
- ✅ `consistent_hash` — Complete (hash ring with vnodes, O(log N) lookup)
- 🔜 `gossip` — Next (membership protocol, phi-accrual failure detector)
- 🔜 `distributed_kv` — After gossip (DynamoDB-style distributed KV)
- 🔜 `rate_limiter` — After gossip + distributed_kv Step 5a

Local testing model: A node is just a process on a distinct port (e.g., 7001, 7002, 7003). Partitioning, replication, and node failures can all be simulated by running multiple processes and killing them locally.

## 📖 History
### Update from transcript 5914962c-1bb8-4e24-801f-8e84349117b1
- Defined the roadmap to a true distributed system (network, persistence, sharding, observability).
- Outlined how to test distributed systems locally using multiple ports and processes.
- Detailed the DynamoDB architecture (smart routing, gossip, hinted handoff) and decided to implement it as a crate.

### Update from transcript e2255da9-fd7e-449a-99ef-9eb7765ed471
- Implemented the `consistent_hash` crate. Crate 3 is now complete.
- Next step is `gossip` (Crate 4), which introduces async/tokio for the first time.
