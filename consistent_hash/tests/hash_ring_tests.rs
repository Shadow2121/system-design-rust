use consistent_hash::HashRing;
use std::collections::HashMap;

#[test]
fn test_determinism() {
    let mut ring = HashRing::new(150);
    ring.add_node("node1");
    ring.add_node("node2");
    ring.add_node("node3");

    let key = "my_key";
    let first_result = ring.get_node(&key).cloned();
    
    // Test determinism by running it multiple times
    for _ in 0..100 {
        assert_eq!(ring.get_node(&key).cloned(), first_result);
    }
}

#[test]
fn test_distribution() {
    let mut ring = HashRing::new(150);
    let nodes = vec!["node1", "node2", "node3", "node4", "node5"];
    for node in &nodes {
        ring.add_node(*node);
    }

    let mut counts = HashMap::new();
    let num_keys = 100_000;
    
    for i in 0..num_keys {
        let key = format!("key{}", i);
        let node = ring.get_node(&key).unwrap();
        *counts.entry(*node).or_insert(0) += 1;
    }

    // Expected keys per node = 20,000. 
    // Let's allow a reasonable tolerance of +/- 20% due to hash randomness
    let expected = num_keys / nodes.len();
    let tolerance = (expected as f64 * 0.20) as usize;
    
    for node in nodes {
        let count = *counts.get(&node).unwrap_or(&0);
        assert!(
            count >= expected - tolerance && count <= expected + tolerance,
            "Node {} has {} keys, expected ~{} (+/- {})",
            node, count, expected, tolerance
        );
    }
}

#[test]
fn test_minimal_movement() {
    let mut ring = HashRing::new(150);
    let nodes = vec!["node1", "node2", "node3", "node4", "node5"];
    for node in &nodes {
        ring.add_node(*node);
    }

    let num_keys = 10_000;
    let mut initial_assignments = HashMap::new();
    
    for i in 0..num_keys {
        let key = format!("key{}", i);
        let node = ring.get_node(&key).unwrap();
        initial_assignments.insert(key, *node);
    }

    // Add a new node
    ring.add_node("node6");

    let mut moved_keys = 0;
    for (key, old_node) in &initial_assignments {
        let new_node = ring.get_node(key).unwrap();
        if new_node != old_node {
            moved_keys += 1;
        }
    }

    // Expected movement when adding 6th node is ~1/6 of keys (approx 1666 keys)
    // We allow a tolerance of +/- 5% of total keys (500)
    let expected_movement = num_keys / 6;
    let tolerance = (num_keys as f64 * 0.05) as usize;
    
    assert!(
        moved_keys >= expected_movement - tolerance && moved_keys <= expected_movement + tolerance,
        "Moved keys: {}, expected ~{} (+/- {})",
        moved_keys, expected_movement, tolerance
    );
}

#[test]
fn test_replica_sets() {
    let mut ring = HashRing::new(150);
    let nodes = vec!["node1", "node2", "node3", "node4", "node5"];
    for node in &nodes {
        ring.add_node(*node);
    }

    let key = "my_important_key";
    
    // Request 3 replicas
    let replicas = ring.get_replicas(&key, 3);
    
    assert_eq!(replicas.len(), 3);
    
    // Check for duplicates
    let mut seen = std::collections::HashSet::new();
    for replica in replicas {
        assert!(seen.insert(replica), "Duplicate node found in replica set");
    }
}

#[test]
fn test_replica_sets_more_than_available() {
    let mut ring = HashRing::new(150);
    ring.add_node("node1");
    ring.add_node("node2");
    
    let key = "my_key";
    
    // Request 3 replicas when only 2 exist
    let replicas = ring.get_replicas(&key, 3);
    
    assert_eq!(replicas.len(), 2);
}

#[test]
fn test_empty_ring() {
    let ring: HashRing<String> = HashRing::new(150);
    assert!(ring.get_node(&"key").is_none());
    assert!(ring.get_replicas(&"key", 3).is_empty());
}
