# Knowledge Index — system-design-rust

| Topic | File | Updated | Summary |
|---|---|---|---|
| KV Store | `kv_store.md` | 2026-07-12 | Complete in-memory cache engine with O(1) arena-based eviction (FIFO/LRU/LFU), split-lock concurrency, generic types. |
| Distributed Architecture | `distributed_architecture.md` | 2026-07-12 | Roadmap for DynamoDB-style distributed KV. kv_store, snowflake, consistent_hash complete. Gossip is next. |
| Dream System | `dream_system.md` | 2026-07-11 | The automated memory consolidation system architecture and tracking mechanism. |
| Consistent Hash | `consistent_hash.md` | 2026-07-12 | Hash ring with virtual nodes, O(log N) binary search lookup, minimal key movement, replica collection. |
| CI Pipeline | `ci_pipeline.md` | 2026-07-12 | GitHub Actions CI configuration. Clippy + tests. Fmt check removed. |
