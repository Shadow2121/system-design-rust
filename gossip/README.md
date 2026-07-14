# Gossip Membership Crate

This crate implements an asynchronous, distributed cluster membership protocol using a UDP-based gossip algorithm and the Phi-Accrual failure detection strategy.

## Status: Complete ✅

This crate is the 4th milestone in the `system-design-rust` distributed systems series. It provides the foundational cluster membership layer required for building the distributed Key-Value store.

## Features

- **Generic Node Identifiers**: Uses generic type `N` (e.g. `String`, `UUID`) bound by `Hash + Eq + Serialize + DeserializeOwned`, making it highly reusable and fully compatible with the `consistent_hash` crate.
- **Asynchronous UDP Networking**: Utilizes `tokio` to run highly concurrent, non-blocking background loops.
- **Phi-Accrual Failure Detector**: Instead of rigid timeouts, this crate mathematically analyzes the historical standard deviation of heartbeat intervals using the Abramowitz and Stegun CDF formula. It accurately distinguishes between true node crashes and temporary network lag.
- **SWIM-style Refutation**: If a node is falsely suspected of being dead due to packet loss, it will automatically refute the suspicion when it receives gossip about its own death by bumping its heartbeat counter.

## Architecture

The system is managed by the `GossipService`, which spins up two background tasks:
1. **The Passive Receiver**: Listens for incoming JSON-serialized `GossipMessage` packets on a UDP socket and merges them into the thread-safe `MembershipTable`.
2. **The Active Gossiper**: Ticks every ~200ms. It increments its local heartbeat, assesses the cluster for failures using the Phi-Accrual math, selects 2 random peers, and broadcasts its entire view of the network state.

*For deeper technical specifics on why UDP was chosen over TCP and the mathematics behind the detector, see `docs/INTERNAL_REFERENCE.md`.*

## Running Tests

To run the full suite of integration tests (multi-node UDP simulation) and unit tests:

```bash
cargo test -p gossip
```
