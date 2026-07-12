use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// A hash ring for consistent hashing.
///
/// It distributes keys uniformly across a set of physical nodes using virtual nodes (vnodes).
pub struct HashRing<N> {
    /// The sorted ring mapping hashed values to physical node identifiers.
    ring: Vec<(u64, N)>,
    /// Tracks active physical nodes and their virtual node count.
    nodes: HashMap<N, usize>,
    /// Number of virtual nodes per physical node by default.
    vnodes_per_node: usize,
}

impl<N> HashRing<N>
where
    N: Clone + Hash + Eq,
{
    /// Creates a new, empty HashRing.
    ///
    /// `vnodes_per_node` determines how many virtual nodes are created per physical node.
    /// A value around 100-200 is typical for good distribution.
    pub fn new(vnodes_per_node: usize) -> Self {
        Self {
            ring: Vec::new(),
            nodes: HashMap::new(),
            vnodes_per_node,
        }
    }

    /// Adds a node to the hash ring.
    ///
    /// The node will be assigned `vnodes_per_node` positions on the ring.
    pub fn add_node(&mut self, node: N) {
        if self.nodes.contains_key(&node) {
            return;
        }

        self.nodes.insert(node.clone(), self.vnodes_per_node);

        for i in 0..self.vnodes_per_node {
            let hash = Self::hash_vnode(&node, i);
            self.insert_vnode(hash, node.clone());
        }
    }

    /// Removes a node from the hash ring.
    pub fn remove_node(&mut self, node: &N) {
        if self.nodes.remove(node).is_none() {
            return;
        }

        self.ring.retain(|(_, n)| n != node);
    }

    /// Returns the node responsible for the given key.
    ///
    /// Returns `None` if the ring is empty.
    pub fn get_node<K: Hash>(&self, key: &K) -> Option<&N> {
        if self.ring.is_empty() {
            return None;
        }

        let hash = Self::hash_key(key);
        
        let pos = self
            .ring
            .binary_search_by_key(&hash, |&(h, _)| h)
            .unwrap_or_else(|pos| pos);

        // Wrap around to the first node if the hash is greater than the highest hash on the ring
        if pos == self.ring.len() {
            Some(&self.ring[0].1)
        } else {
            Some(&self.ring[pos].1)
        }
    }

    /// Returns a list of up to `n` distinct replica nodes for the given key.
    ///
    /// Walks the ring starting from the key's position to collect `n` unique physical nodes.
    /// If there are fewer than `n` distinct nodes in the ring, it returns all available nodes.
    pub fn get_replicas<K: Hash>(&self, key: &K, n: usize) -> Vec<N> {
        if self.ring.is_empty() || n == 0 {
            return Vec::new();
        }

        let hash = Self::hash_key(key);
        let start_pos = self
            .ring
            .binary_search_by_key(&hash, |&(h, _)| h)
            .unwrap_or_else(|pos| pos);

        let mut replicas = Vec::with_capacity(n.min(self.nodes.len()));
        let mut seen = HashSet::with_capacity(n.min(self.nodes.len()));

        let ring_len = self.ring.len();
        for i in 0..ring_len {
            if replicas.len() == n || replicas.len() == self.nodes.len() {
                break;
            }

            let pos = (start_pos + i) % ring_len;
            let node = &self.ring[pos].1;

            if seen.insert(node.clone()) {
                replicas.push(node.clone());
            }
        }

        replicas
    }

    /// Helper function to hash a virtual node
    fn hash_vnode(node: &N, vnode_idx: usize) -> u64 {
        let mut hasher = DefaultHasher::new();
        node.hash(&mut hasher);
        vnode_idx.hash(&mut hasher);
        hasher.finish()
    }

    /// Helper function to hash a key
    fn hash_key<K: Hash>(key: &K) -> u64 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }

    /// Inserts a vnode into the sorted ring
    fn insert_vnode(&mut self, hash: u64, node: N) {
        let pos = self
            .ring
            .binary_search_by_key(&hash, |&(h, _)| h)
            .unwrap_or_else(|pos| pos);
        self.ring.insert(pos, (hash, node));
    }
}
