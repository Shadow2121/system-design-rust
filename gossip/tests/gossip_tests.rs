use gossip::{GossipService, NodeStatus};
use std::net::SocketAddr;
use std::time::Duration;

#[tokio::test]
async fn test_discovery() {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    
    let node1 = GossipService::spawn("node1".to_string(), addr).await;
    let node2 = GossipService::spawn("node2".to_string(), addr).await;
    let node3 = GossipService::spawn("node3".to_string(), addr).await;

    // Seed node1 with node2
    node1.add_seed_node("node2".to_string(), node2.local_address).await;
    // Seed node2 with node3
    node2.add_seed_node("node3".to_string(), node3.local_address).await;

    // Wait for gossip to propagate
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let entries1 = node1.get_entries().await;
    assert!(entries1.contains_key("node2"));
    assert!(entries1.contains_key("node3"), "Node 1 should have discovered Node 3 via gossip");
    assert_eq!(entries1.get("node3").unwrap().status, NodeStatus::Alive);
}

#[tokio::test]
async fn test_failure_detection() {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    
    let node1 = GossipService::spawn("nodeA".to_string(), addr).await;
    let node2_addr = {
        // node2 exists only in this block
        let node2 = GossipService::spawn("nodeB".to_string(), addr).await;
        node1.add_seed_node("nodeB".to_string(), node2.local_address).await;
        
        // Let them gossip a bit to collect enough heartbeat interval samples (min 5)
        tokio::time::sleep(Duration::from_millis(1500)).await;
        
        let entries = node1.get_entries().await;
        assert_eq!(entries.get("nodeB").unwrap().status, NodeStatus::Alive);
        
        node2.local_address
        // node2 is dropped here, its tasks are aborted.
    };

    // Wait for failure detector to kick in (threshold = 8.0, timeout = 2s)
    tokio::time::sleep(Duration::from_millis(3500)).await;

    let entries = node1.get_entries().await;
    let status = entries.get("nodeB").unwrap().status;
    assert!(status == NodeStatus::Suspected || status == NodeStatus::Dead, "Node B status was {:?}", status);
}

#[tokio::test]
async fn test_refutation() {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let node1 = GossipService::spawn("nodeA".to_string(), addr).await;
    let node2 = GossipService::spawn("nodeB".to_string(), addr).await;
    
    node1.add_seed_node("nodeB".to_string(), node2.local_address).await;
    node2.add_seed_node("nodeA".to_string(), node1.local_address).await;
    
    // Give time to stabilize
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Simulate node1 thinking nodeB is Suspected (network hiccup)
    {
        let mut t = node1.table.write().await;
        if let Some(state) = t.entries.get_mut("nodeB") {
            state.status = NodeStatus::Suspected;
        }
    }
    
    // Node1 will gossip this suspicion to Node2. 
    // Node2 should see it, refute it by updating its own heartbeat, and broadcast it back.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    
    let entries = node1.get_entries().await;
    // nodeB should be Alive again in node1's table
    assert_eq!(entries.get("nodeB").unwrap().status, NodeStatus::Alive, "Node B should have refuted its suspicion");
}
