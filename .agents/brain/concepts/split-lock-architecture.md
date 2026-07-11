# Split-Lock Architecture

In high-performance caching (like Caffeine, Moka, or our `kv_store`), a single global lock (even an `RwLock`) becomes a massive bottleneck. 

The primary issue is that **reads are actually writes** in a cache: when you access a key, you must update the eviction policy (e.g., bump it to the front of the LRU queue). Because policy mutation requires an exclusive lock, all concurrent reads are forced to serialize.

**The Solution:**
Decouple the data plane from the control plane.
1. The **Data Plane** (the HashMap) is protected by an `RwLock`.
2. The **Control Plane** (the Eviction Policy) is protected by a separate `Mutex`.

During a read, the thread takes a shared read-lock on the data, retrieves the value concurrently with thousands of others, and only takes an exclusive lock on the policy for a few microseconds to update the tracking queue. This allows massive read concurrency while maintaining accurate cache eviction.
