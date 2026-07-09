use snowflake_gen::SnowflakeGenerator;
use std::sync::Arc;
use std::thread;

#[test]
fn test_public_api_single_thread() {
    // 1. We test that the public constructor works
    let generator = SnowflakeGenerator::new(5).expect("Public API failed to initialize");

    // 2. We test that the public generate_id method works
    let id1 = generator.generate_id().expect("Failed to generate first ID");
    let id2 = generator.generate_id().expect("Failed to generate second ID");

    // 3. We verify basic sequential logic through the public boundary
    assert!(id2 > id1, "Subsequent IDs should be strictly greater in the same millisecond");
}

#[test]
fn test_public_api_multi_thread_sharing() {
    // This test proves that our struct safely implements standard Rust trait bounds 
    // (like Send and Sync) required for consumers to use it across threads.
    let generator = Arc::new(SnowflakeGenerator::new(10).unwrap());
    let mut handles = vec![];

    for _ in 0..4 {
        let gen_clone = Arc::clone(&generator);
        let handle = thread::spawn(move || {
            // Consumer safely generates IDs in a concurrent environment
            let id = gen_clone.generate_id().unwrap();
            assert!(id > 0);
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().is_ok(), "Consumer thread panicked");
    }
}

#[test]
fn test_multi_node_cluster_uniqueness() {
    let mut handles = vec![];

    // Simulate 3 separate servers in a distributed cluster
    for machine_id in 1..=3 {
        let handle = thread::spawn(move || {
            // Each "server" gets its own generator with a unique Machine ID
            let generator = SnowflakeGenerator::new(machine_id).unwrap();
            let mut node_ids = vec![];
            
            // Each server generates 1,000 IDs as fast as possible
            for _ in 0..1_000 {
                node_ids.push(generator.generate_id().unwrap());
            }
            node_ids
        });
        handles.push(handle);
    }

    // Collect all IDs from all servers into a single global HashSet
    let mut global_ids = std::collections::HashSet::new();
    for handle in handles {
        let node_ids = handle.join().expect("Node thread panicked");
        for id in node_ids {
            assert!(
                global_ids.insert(id),
                "GLOBAL COLLISION! Two different nodes generated the exact same ID: {}", id
            );
        }
    }
    
    // 3 servers * 1,000 IDs = 3,000 completely unique IDs
    assert_eq!(global_ids.len(), 3_000);
}