# Consistent Hashing

The core problem: how do you decide **which server owns which key** in a distributed system, and how do you do it so that adding or removing a server doesn't reshuffle *everything*?

## The Naive Approach (and Why It Fails)
`server = hash(key) % num_servers`

This is what you'd do first. It works great until a server dies. Then `num_servers` changes, and suddenly **every single key** maps to a different server. Your entire cache is invalidated. Catastrophic.

## The Hash Ring
Imagine a circle (ring) from 0 to 2^64. You hash each server onto this ring at some position. To find which server owns a key, you hash the key and walk **clockwise** until you hit the first server. That server owns the key.

Now when a server is added, only the keys between it and its predecessor need to move — roughly **1/N** of the total. Everything else stays put.

## Virtual Nodes (The Real Magic)
In practice, hashing 5 servers onto a ring gives you 5 unevenly-spaced points. One server might accidentally "own" 40% of the ring. The fix: give each physical server ~150 **virtual** positions on the ring. Now instead of 5 points, you have 750, and the law of large numbers kicks in to smooth the distribution.

Think of it like placing 5 pins on a dartboard — you might cluster them. But placing 750 pins? They'll be roughly evenly spaced.

## In Our Implementation
- **Ring**: `Vec<(u64, NodeId)>` kept sorted. Lookup via `binary_search_by_key` = **O(log N)**.
- **Replicas**: Walk clockwise collecting **distinct physical nodes** (skipping duplicate vnodes). This is how [[distributed-kv|distributed_kv]] will find replication targets.
- **Tests prove**: Determinism, ~20% tolerance on distribution, ~1/N key movement on add, no duplicate replicas.

## Connection to DynamoDB / Riak
Both DynamoDB and Riak use consistent hashing as the routing layer. When a request comes in:
1. Hash the key → find the owner node on the ring
2. Find N successor nodes for replication
3. Write to W of them (write quorum)
4. Read from R of them (read quorum)

Our `consistent_hash` crate handles steps 1-2. Steps 3-4 come in [[distributed-kv|distributed_kv]].
