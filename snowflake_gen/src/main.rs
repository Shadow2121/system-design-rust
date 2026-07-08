use std::sync::Arc;
use std::thread;
use std::sync::mpsc;
use std::time::Duration;

// Import our Snowflake Generator from Phase 1
use snowflake_gen::SnowflakeGenerator;

// This struct represents a row in our database
#[derive(Debug)]
struct DatabaseRecord {
    internal_id: u64,
    data: String,
}

fn main() {
    println!("Starting Hybrid ID System (Snowflake + Gapless)...\n");

    let generator = Arc::new(SnowflakeGenerator::new(1).unwrap());

    // 1. Create the MPSC Channel (The Event Queue)
    // tx = Transmitter (Producer), rx = Receiver (Consumer)
    let (tx, rx) = mpsc::channel::<DatabaseRecord>();

    // 2. Spawn the Background Worker (The Slow Path / Single Consumer)
    // This thread acts as our gapless sequence assigner.
    let worker_handle = thread::spawn(move || {
        let mut gapless_counter = 1000; // Starting business ID

        println!("[Worker] Started listening for new records...");

        // The worker pulls records off the queue one by one.
        // Because it's a single thread, there are zero race conditions for the counter.
        for record in rx {
            gapless_counter += 1;
            let invoice_id = format!("INV-{}", gapless_counter);
            
            // Simulate database update latency
            thread::sleep(Duration::from_millis(50));
            
            println!(
                "[Worker] Assigned Gapless ID: {} to Internal Snowflake ID: {}",
                invoice_id, record.internal_id
            );
        }
        println!("[Worker] Queue closed. Shutting down.");
    });

    // 3. Spawn Concurrent Web Threads (The Fast Path / Multiple Producers)
    let mut web_threads = vec![];

    for thread_id in 0..3 {
        let gen_clone = Arc::clone(&generator);
        
        // Clone the transmitter so multiple threads can send to the same queue
        let tx_clone = tx.clone();

        let handle = thread::spawn(move || {
            for order_num in 0..3 {
                // Instantly generate the internal Snowflake ID
                let internal_id = gen_clone.generate_id().unwrap();
                
                println!(
                    "[Web API {}] Generated Internal ID: {} for Order {}", 
                    thread_id, internal_id, order_num
                );

                // Simulate saving to the DB with a NULL invoice_id, 
                // then send it to the background worker via the channel
                let record = DatabaseRecord {
                    internal_id,
                    data: format!("Order Payload {}", order_num),
                };
                
                tx_clone.send(record).unwrap();
            }
        });
        web_threads.push(handle);
    }

    // 4. Clean up
    // We must drop the original transmitter, or the receiver loop will wait forever
    drop(tx); 

    // Wait for all web threads to finish handling requests
    for handle in web_threads {
        handle.join().unwrap();
    }

    // Wait for the background worker to finish processing the queue
    worker_handle.join().unwrap();
    
    println!("\nSystem shut down cleanly.");
}