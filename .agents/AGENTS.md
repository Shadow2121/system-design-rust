# Workspace Context — system-design-rust

## Memory System — READ FIRST

### On Every Conversation Start
1. Check if `.agents/transcripts/pending.txt` exists.
   If YES → run the dream skill at `.agents/skills/dream/SKILL.md` BEFORE anything else.
2. Read `.agents/knowledge/INDEX.md`.
3. Load only the KI files relevant to the current task (do NOT load all — use INDEX to select).

## Purpose
This Rust workspace implements system design concepts from scratch. Each crate is a
self-contained, well-tested implementation of **one concept**. The goal is deep understanding,
not production-grade software. Code quality should be high; operational scale is not the goal.

---

## Crate Status

| Crate | Status | Concept |
|---|---|---|
| `kv_store` | ✅ Complete | Cache engine: eviction policies (FIFO/LRU/LFU O(1)), lazy TTL, split-lock concurrency |
| `snowflake_gen` | ✅ Complete | Distributed unique ID generation |
| `consistent_hash` | 🔜 Next | Hash ring, virtual nodes, key routing, minimal reshuffling on topology change |
| `gossip` | 🔜 After consistent_hash | Membership protocol, phi-accrual failure detector, node discovery |
| `distributed_kv` | 🔜 After gossip | Full DynamoDB-style distributed KV: quorum, replication, hinted handoff, read repair |
| `rate_limiter` | 🔜 After gossip + distributed_kv Step 5a | Distributed rate limiting: token bucket, gossip-based approximate global counts |

---

## kv_store — Decisions & Architecture (COMPLETE, DO NOT REDESIGN)

- **Pure sync library crate.** No networking will be added to `kv_store` itself.
- Uses `std::sync::RwLock` (data plane) + `std::sync::Mutex` (policy plane) — the split-lock architecture.
- `KvStore<K, V, P>` — single-threaded, mutable, for use within a single thread.
- `ConcurrentKvStore<K, V, P>` — thread-safe wrapper using `Arc`, shareable across OS threads.
- Three eviction policies, all **O(1)** for every operation:
  - `FifoPolicy` — VecDeque
  - `LruPolicy` — Arena-based doubly linked list (index-as-pointer trick, zero unsafe)
  - `LfuPolicy` — Arena-based frequency buckets with shared arena
- Lazy TTL: expiry checked on `get()`, no background GC thread.
- Ghost-key prevention: `on_delete()` is called on every removal path.
- The only possible future addition: an optional `tokio` Cargo feature flag that swaps
  `std::sync` for `tokio::sync` equivalents, enabling use inside async contexts without
  `spawn_blocking`. This is not yet needed.

---

## Full Build Order & Plan

```
kv_store ✅  →  consistent_hash  →  gossip  →  distributed_kv  →  rate_limiter
snowflake ✅                              (steps 5a → 5b → 5c → 5d → 5e → 5f)
```

### Crate 3: `consistent_hash`
**No async, no networking. Pure math and data structures.**

Implement:
- Hash ring with virtual nodes (vnodes, ~150 per physical node for even distribution)
- `get_node(key) -> NodeId` — O(log N) binary search on sorted ring
- `add_node` / `remove_node` — proves minimal key movement (~1/N of keys affected)
- `get_replicas(key, n) -> Vec<NodeId>` — N successive distinct nodes for replication

Tests must prove:
- Adding a node moves only ~1/N keys
- Same key always maps to same node (determinism)
- Key distribution is roughly even
- Replica sets don't contain duplicate nodes

Dependencies: none (pure Rust)

---

### Crate 4: `gossip`
**First crate to use `tokio`. Introduces async.**

Implement:
- `MembershipTable`: `HashMap<NodeId, NodeState>` where `NodeState = { address, status: Alive/Suspected/Dead, heartbeat_counter, last_seen }`
- Gossip round: every ~200ms, pick 2 random peers, send full membership table over UDP/TCP
- Merge logic: on receive, update own table with higher heartbeat counters
- Phi-accrual failure detector: probability-based, avoids false positives from slow nodes
- Suspicion: Alive → Suspected → Dead

Tests must prove:
- New node joining is discovered by all others within a few gossip rounds
- A node that stops heartbeating is eventually marked Dead
- Minority partition marks majority nodes as Dead and vice versa

Dependencies: `tokio`, `serde`, `serde_json`

---

### Crate 5: `distributed_kv`
**The main dish. DynamoDB-style distributed KV store.**

Has a binary target: `src/bin/node.rs`
CLI flags: `--port`, `--node-id`, `--peers`

**Step 5a — Single Node HTTP**
- `tokio` + `axum` binary exposing `GET/PUT/DELETE /keys/:key`, `GET /health`
- Use `kv_store::ConcurrentKvStore` for local storage
- Use `tokio::sync` or `spawn_blocking` for the sync kv_store calls
- Test with `curl`

**Step 5b — Multi-Node Routing**
- Integrate `consistent_hash`
- Route requests: hash key → find owner node → forward if not local
- Test: 3 nodes, key "alice" always routes to node 1

**Step 5c — Replication**
- On write: store locally + forward to N-1 successors on the ring
- Configurable N (replication factor, default 3), W (write quorum, default 2)
- Return OK when W nodes acknowledge
- Test: kill a replica, data still readable

**Step 5d — Gossip Integration**
- Integrate `gossip` crate — replace static `--peers` config with live membership
- Router skips dead nodes from gossip table
- Test: self-organizing cluster, no static config needed

**Step 5e — Hinted Handoff**
- If target node is down, write to a hint node with `{ intended_for, key, value }`
- When target recovers, hint node forwards buffered writes
- Test: kill node, write, restart node, data heals

**Step 5f — Read Repair**
- On quorum read, if replicas disagree, async write the latest version to stale replicas
- Test: manually corrupt a replica, do a read, stale replica self-heals

Dependencies: `kv_store`, `consistent_hash`, `gossip`, `tokio`, `axum`, `serde`, `serde_json`

**Deliberately skipped** (each is its own deep topic / future crate):
- Raft/Paxos consensus → future `raft` crate
- Merkle tree anti-entropy → future topic
- WAL + disk persistence → future `wal` crate
- CRDT conflict resolution → future `crdt` crate

---

### Crate 6: `rate_limiter`
**Minimum prerequisite: `gossip` crate + `distributed_kv` Step 5a only.**

Implement two strategies:
1. **Centralized:** One designated limiter node holds all counts. Simple, has SPOF. Good for learning.
2. **Gossip-based:** Each node tracks local count. Gossip counts to peers. Global estimate = sum.
   Approximate but fully distributed. No SPOF.

Also implement:
- Fixed window counter
- Sliding window counter (more accurate)
- Token bucket

Test: 3 nodes, 100 req/s global limit, flood all 3 simultaneously, verify global rate is
respected within ~5% margin.

---

## Conventions
- Every crate has unit tests inline (`#[cfg(test)]`) and integration tests in `tests/`
- Every crate has a `docs/INTERNAL_REFERENCE.md` explaining architecture decisions
- No `unsafe` code unless absolutely unavoidable and clearly documented
- Prefer correctness and clarity over micro-optimization
- Generic types over concrete types wherever it makes sense

## Local Testing Model for Distributed Concepts
- A "node" = a running OS process with a unique port and optional data directory
- Cluster test = integration test that spawns N processes via `std::process::Command`
- Crash simulation = call `.kill()` on a `Child` process handle
- Recovery simulation = respawn the process
- Network partition = use a local TCP proxy or Windows firewall rules to block ports
- Geographic latency = `tokio::time::sleep()` injected into message handlers
