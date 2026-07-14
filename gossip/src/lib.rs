pub mod detector;

use detector::PhiAccrualDetector;
use rand::seq::IteratorRandom;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::HashMap,
    hash::Hash,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::UdpSocket,
    sync::RwLock,
    time,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Alive,
    Suspected,
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    pub address: SocketAddr,
    pub status: NodeStatus,
    pub heartbeat_counter: u64,
    #[serde(skip)]
    pub last_seen: Option<Instant>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(bound = "N: Eq + Hash + Clone + Serialize + DeserializeOwned")]
pub struct GossipMessage<N>
where
    N: Hash + Eq,
{
    pub sender_id: N,
    pub entries: HashMap<N, NodeState>,
}

pub struct MembershipTable<N> {
    pub local_id: N,
    pub entries: HashMap<N, NodeState>,
    pub detectors: HashMap<N, PhiAccrualDetector>,
}

// `impl<N>` begins the implementation block for our generic struct.
// The `where` clause enforces "trait bounds" on the generic type `N`.
// For example, `Send + Sync` means it is safe to transfer and share `N` across threads.
impl<N> MembershipTable<N>
where
    N: Clone + Hash + Eq + Send + Sync,
{
    pub fn new(local_id: N, local_address: SocketAddr) -> Self {
        let mut entries = HashMap::new();
        entries.insert(
            local_id.clone(),
            NodeState {
                address: local_address,
                status: NodeStatus::Alive,
                heartbeat_counter: 1,
                last_seen: Some(Instant::now()),
            },
        );

        Self {
            local_id,
            entries,
            detectors: HashMap::new(),
        }
    }

    pub fn get_entries(&self) -> HashMap<N, NodeState> {
        self.entries.clone()
    }

    pub fn update_heartbeat(&mut self) {
        if let Some(state) = self.entries.get_mut(&self.local_id) {
            state.heartbeat_counter += 1;
            state.status = NodeStatus::Alive;
        }
    }

    pub fn merge(&mut self, incoming_entries: HashMap<N, NodeState>) {
        let now = Instant::now();

        for (id, incoming_state) in incoming_entries {
            if id == self.local_id {
                // Ignore our own broadcasted state, unless someone thinks we are suspected/dead!
                if incoming_state.status != NodeStatus::Alive && incoming_state.heartbeat_counter == self.entries.get(&self.local_id).unwrap().heartbeat_counter {
                    self.update_heartbeat(); // Refute
                }
                continue;
            }

            let local_state = self.entries.get_mut(&id);
            match local_state {
                None => {
                    // New node discovered
                    if incoming_state.status != NodeStatus::Dead {
                        let mut new_state = incoming_state.clone();
                        new_state.last_seen = Some(now);
                        self.entries.insert(id.clone(), new_state);
                        
                        let mut detector = PhiAccrualDetector::new();
                        detector.heartbeat_received(now);
                        self.detectors.insert(id, detector);
                    }
                }
                Some(local) => {
                    if incoming_state.heartbeat_counter > local.heartbeat_counter {
                        local.heartbeat_counter = incoming_state.heartbeat_counter;
                        local.status = incoming_state.status;
                        local.last_seen = Some(now);
                        local.address = incoming_state.address;
                        
                        if let Some(detector) = self.detectors.get_mut(&id) {
                            detector.heartbeat_received(now);
                        }
                    } else if incoming_state.heartbeat_counter == local.heartbeat_counter {
                        // Same heartbeat, adopt worse status if necessary
                        if incoming_state.status == NodeStatus::Dead && local.status != NodeStatus::Dead {
                            local.status = NodeStatus::Dead;
                        } else if incoming_state.status == NodeStatus::Suspected && local.status == NodeStatus::Alive {
                            local.status = NodeStatus::Suspected;
                        }
                    }
                }
            }
        }
    }

    pub fn check_failures(&mut self, phi_threshold: f64, dead_timeout: Duration) {
        let now = Instant::now();
        for (id, state) in self.entries.iter_mut() {
            if id == &self.local_id || state.status == NodeStatus::Dead {
                continue;
            }

            if let Some(detector) = self.detectors.get(id) {
                let phi = detector.phi(now);
                if phi > phi_threshold {
                    if state.status == NodeStatus::Alive {
                        state.status = NodeStatus::Suspected;
                    }
                }
                
                // If suspected for too long
                if state.status == NodeStatus::Suspected {
                    if let Some(last_seen) = state.last_seen {
                        if now.duration_since(last_seen) > dead_timeout {
                            state.status = NodeStatus::Dead;
                        }
                    }
                }
            }
        }
    }

    pub fn pick_random_peers(&self, count: usize) -> Vec<SocketAddr> {
        let mut rng = rand::thread_rng();
        self.entries
            .iter()
            .filter(|(id, state)| *id != &self.local_id && state.status != NodeStatus::Dead)
            .map(|(_, state)| state.address)
            .choose_multiple(&mut rng, count)
    }
}

pub struct GossipService<N> {
    // `Arc` (Atomic Reference Counted) allows multiple threads to safely share ownership of the data.
    // `RwLock` (Read-Write Lock) allows many readers to access the table at once, but only one writer.
    // Together, `Arc<RwLock<T>>` is the standard Rust pattern for shared, mutable state across threads.
    pub table: Arc<RwLock<MembershipTable<N>>>,
    pub local_address: SocketAddr,
    
    // `JoinHandle` is a handle to a background task (similar to a thread handle).
    // We store these so we can cleanly abort the background loops when the service shuts down.
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl<N> GossipService<N>
where
    // `'static` means the type `N` contains no short-lived references; it owns all its data.
    // This is required when moving data into background threads (`tokio::spawn`).
    N: Clone + Hash + Eq + Send + Sync + Serialize + DeserializeOwned + std::fmt::Debug + 'static,
{
    pub async fn spawn(local_id: N, address: SocketAddr) -> Self {
        let socket = Arc::new(UdpSocket::bind(address).await.unwrap());
        let local_address = socket.local_addr().unwrap();
        
        let table = Arc::new(RwLock::new(MembershipTable::new(local_id.clone(), local_address)));
        
        // We clone the `Arc` pointers. This does not copy the underlying data; 
        // it just increments the reference count so the new task can have its own pointer to the table.
        let table_rx = table.clone();
        let socket_rx = socket.clone();
        
        // `tokio::spawn` fires off a green thread (an async background task).
        // `async move` tells Rust to move ownership of `table_rx` and `socket_rx` into the new task.
        let h1 = tokio::spawn(async move {
            // Stack-allocated fixed-size buffer for receiving UDP packets. `[0u8; N]` means an array of zeroes of size N.
            let mut buf = [0u8; 65536];
            loop {
                // `.await` yields execution back to the Tokio runtime until a UDP packet actually arrives.
                // This prevents the loop from burning CPU while waiting.
                if let Ok((len, _addr)) = socket_rx.recv_from(&mut buf).await {
                    if let Ok(msg) = serde_json::from_slice::<GossipMessage<N>>(&buf[..len]) {
                        // `.write().await` acquires an exclusive lock on the table so we can mutate it safely.
                        let mut t = table_rx.write().await;
                        t.merge(msg.entries);
                    }
                }
            }
        });

        let table_tx = table.clone();
        let socket_tx = socket.clone();
        let h2 = tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_millis(200));
            loop {
                interval.tick().await;
                
                let (msg, peers) = {
                    let mut t = table_tx.write().await;
                    t.update_heartbeat();
                    t.check_failures(8.0, Duration::from_secs(2)); // Reduced for faster tests
                    
                    let msg = GossipMessage {
                        sender_id: t.local_id.clone(),
                        entries: t.get_entries(),
                    };
                    
                    (msg, t.pick_random_peers(2))
                };

                if let Ok(serialized) = serde_json::to_vec(&msg) {
                    for peer in peers {
                        let _ = socket_tx.send_to(&serialized, peer).await;
                    }
                }
            }
        });

        Self { table, local_address, tasks: vec![h1, h2] }
    }

    pub async fn get_entries(&self) -> HashMap<N, NodeState> {
        self.table.read().await.get_entries()
    }
    
    /// Used for seeding the node initially
    pub async fn add_seed_node(&self, id: N, address: SocketAddr) {
        let mut t = self.table.write().await;
        let mut seed_entries = HashMap::new();
        seed_entries.insert(id, NodeState {
            address,
            status: NodeStatus::Alive,
            heartbeat_counter: 1,
            last_seen: Some(Instant::now()),
        });
        t.merge(seed_entries);
    }
}

impl<N> Drop for GossipService<N> {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_updates_heartbeat() {
        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let mut table = MembershipTable::new("node_a".to_string(), addr);
        
        let mut incoming = HashMap::new();
        incoming.insert("node_b".to_string(), NodeState {
            address: "127.0.0.1:8001".parse().unwrap(),
            status: NodeStatus::Alive,
            heartbeat_counter: 10,
            last_seen: None,
        });

        table.merge(incoming);
        
        let entries = table.get_entries();
        assert_eq!(entries.get("node_b").unwrap().heartbeat_counter, 10);
        
        // Now merge an older heartbeat (should be ignored)
        let mut incoming_old = HashMap::new();
        incoming_old.insert("node_b".to_string(), NodeState {
            address: "127.0.0.1:8001".parse().unwrap(),
            status: NodeStatus::Alive,
            heartbeat_counter: 5,
            last_seen: None,
        });
        
        table.merge(incoming_old);
        let entries = table.get_entries();
        assert_eq!(entries.get("node_b").unwrap().heartbeat_counter, 10, "Older heartbeat should be ignored by the merge logic");
    }
}
