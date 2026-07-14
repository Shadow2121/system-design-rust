# Architecture Decisions: `gossip`

## Overview
This crate provides an asynchronous, UDP-based gossip membership protocol. It introduces `tokio` to the workspace, providing a foundational cluster membership and failure detection layer for the distributed KV store.

## Key Design Choices

### 1. UDP Gossip Protocol
- **Why UDP over TCP?** Gossip involves constant heartbeat chatter between random peers every ~200ms. Establishing TCP handshakes for every exchange would exhaust file descriptors and create massive latency overhead. UDP allows fire-and-forget datagrams, which is perfect for weakly-consistent eventually-consistent state sharing.
- **Message Format**: We use `serde_json` for serialization. The entire `MembershipTable` is serialized. In a production system with 10,000 nodes, we would exchange deltas, but for our scale, full table exchanges keep the merge logic robust and simple.

### 2. Phi-Accrual Failure Detector
- Instead of using a static heartbeat timeout (e.g., "if no heartbeat in 5s, mark dead"), we use a sliding window of the last 100 intervals to calculate the mean and standard deviation of arrival times.
- **Phi Math**: $\Phi = -\log_{10}(1 - \text{CDF}(x))$
- We approximate the normal distribution CDF using the Abramowitz and Stegun formula, avoiding external dependencies like `statrs`.
- A threshold of `8.0` is used to transition a node to `Suspected`.

### 3. SWIM-style Refutation
- If a node is marked `Suspected` due to packet loss, it will eventually receive a gossip message indicating its own suspected status.
- It refutes this by incrementing its heartbeat counter and broadcasting itself as `Alive`. Because the heartbeat is higher than what the cluster has, the cluster adopts the `Alive` state again.

### 4. Generic Node IDs
- `MembershipTable<N>` uses a generic `N` where `N: Hash + Eq + Clone + Serialize + DeserializeOwned`. This mirrors the `consistent_hash` crate, allowing the application layer to define what a Node ID is (e.g., a `String` hostname or a UUID).

### 5. Tokio Background Tasks
- `GossipService` manages two background loops:
  1. **Receiver**: Parses incoming UDP packets and merges them into the `MembershipTable`.
  2. **Gossiper**: Ticks every 200ms, evaluates failures via Phi-Accrual, and sends the table to 2 random active peers.
- The tasks are stored as `JoinHandle`s and are cleanly aborted in the `Drop` implementation to enable graceful shutdown and clean integration testing.
