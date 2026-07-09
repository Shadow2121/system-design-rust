use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// --- 1. Constants (The Blueprint) ---
// Custom Epoch: Jan 1, 2024, 00:00:00 UTC (in milliseconds)
const CUSTOM_EPOCH: u64 = 1_704_067_200_000;

const MACHINE_ID_BITS: u64 = 10;
const SEQUENCE_BITS: u64 = 12;

const MAX_MACHINE_ID: u64 = (1 << MACHINE_ID_BITS) - 1; // 1023
const MAX_SEQUENCE: u64 = (1 << SEQUENCE_BITS) - 1; // 4095

const MACHINE_ID_SHIFT: u64 = SEQUENCE_BITS; // Shift by 12
const TIMESTAMP_SHIFT: u64 = SEQUENCE_BITS + MACHINE_ID_BITS; // Shift by 22

const MAX_DRIFT_TOLERANCE_MS: u64 = 5;

// --- 2. Error Handling ---
#[derive(Debug, PartialEq)]
pub enum SnowflakeError {
    MachineIdOutOfBounds,
    ClockMovedBackwards { drift_ms: u64 },
}

// --- 3. State Management ---
#[derive(Debug)]
struct SnowflakeState {
    last_timestamp: u64,
    sequence: u64,
}

#[derive(Debug)]
pub struct SnowflakeGenerator {
    machine_id: u64,
    // The state is wrapped in a Mutex for thread-safe mutation
    state: Mutex<SnowflakeState>,
}

// --- 4. The Generation Algorithm ---
impl SnowflakeGenerator {
    /// Initialize a new generator with a specific machine ID.
    pub fn new(machine_id: u64) -> Result<Self, SnowflakeError> {
        if machine_id > MAX_MACHINE_ID {
            return Err(SnowflakeError::MachineIdOutOfBounds);
        }

        Ok(SnowflakeGenerator {
            machine_id,
            state: Mutex::new(SnowflakeState {
                last_timestamp: 0,
                sequence: 0,
            }),
        })
    }

    /// Helper to get current time in MS since our custom epoch.
    fn current_time_ms() -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards before Unix Epoch!");
        now.as_millis() as u64 - CUSTOM_EPOCH
    }

    /// The core generation logic (The Critical Section)
    pub fn generate_id(&self) -> Result<u64, SnowflakeError> {
        // 1. Lock the state. In Rust, you can't access the inner data without locking it.
        // If another thread panics while holding the lock, we unwrap() and panic too (poisoned lock).
        let mut state = self.state.lock().unwrap();

        let mut current_time = Self::current_time_ms();

        // 2. Handle Clock Drift (The Staff Engineer Edge Case)
        if current_time < state.last_timestamp {
            let drift = state.last_timestamp - current_time;

            if drift <= MAX_DRIFT_TOLERANCE_MS {
                // Small drift: Wait it out
                thread::sleep(Duration::from_millis(drift));
                current_time = Self::current_time_ms();
            } else {
                // Large drift: Fail fast to protect the system
                return Err(SnowflakeError::ClockMovedBackwards { drift_ms: drift });
            }
        }

        // 3. Sequence Generation
        if current_time == state.last_timestamp {
            // Same millisecond: increment sequence
            // The bitwise AND (& MAX_SEQUENCE) guarantees we roll over to 0 if we hit 4096
            state.sequence = (state.sequence + 1) & MAX_SEQUENCE;

            // If sequence rolled over to 0, we exhausted this millisecond's IDs
            if state.sequence == 0 {
                // Spin wait until the next millisecond
                loop {
                    current_time = Self::current_time_ms();
                    if current_time > state.last_timestamp {
                        break;
                    }
                }
            }
        } else {
            // Next millisecond: reset sequence
            state.sequence = 0;
        }

        // 4. Update state and release lock
        // (Lock is automatically released when `state` goes out of scope at the end of the function)
        state.last_timestamp = current_time;

        // 5. Assemble and Return the 64-bit ID
        let id = (current_time << TIMESTAMP_SHIFT)
            | (self.machine_id << MACHINE_ID_SHIFT)
            | state.sequence;

        Ok(id)
    }
}

// --- 5. Test Suite ---
// The #[cfg(test)] annotation tells the Rust compiler to ONLY compile
// this module when you run `cargo test`. It will be ignored in production builds.
#[cfg(test)]
mod tests {
    // Import everything from the outer module (our generator logic)
    use super::*;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_valid_machine_id_initialization() {
        // 1023 is our MAX_MACHINE_ID
        let generator = SnowflakeGenerator::new(1023);
        assert!(
            generator.is_ok(),
            "Generator should initialize with valid ID"
        );
    }

    #[test]
    fn test_invalid_machine_id_rejection() {
        // 1024 is out of bounds (requires 11 bits)
        let generator = SnowflakeGenerator::new(1024);
        assert_eq!(
            generator.unwrap_err(),
            SnowflakeError::MachineIdOutOfBounds,
            "Generator should reject machine IDs > 1023"
        );
    }

    #[test]
    fn test_sequential_generation_uniqueness() {
        let generator = SnowflakeGenerator::new(1).expect("Failed to init");
        let mut generated_ids = HashSet::new();

        // Generate 10,000 IDs in a tight loop
        for _ in 0..10_000 {
            let id = generator.generate_id().expect("Generation failed");

            // HashSet::insert returns false if the value was already present
            assert!(
                generated_ids.insert(id),
                "Duplicate ID generated in sequential loop: {}",
                id
            );
        }

        assert_eq!(generated_ids.len(), 10_000);
    }

    #[test]
    fn test_highly_concurrent_uniqueness() {
        let generator = Arc::new(SnowflakeGenerator::new(42).unwrap());
        let mut handles = vec![];

        // Spawn 20 concurrent threads
        for _ in 0..20 {
            let gen_clone = Arc::clone(&generator);

            let handle = thread::spawn(move || {
                let mut local_ids = vec![];
                // Each thread generates 1,000 IDs
                for _ in 0..1_000 {
                    local_ids.push(gen_clone.generate_id().unwrap());
                }
                local_ids
            });
            handles.push(handle);
        }

        // Collect all IDs from all threads into a single HashSet to check for collisions
        let mut all_ids = HashSet::new();
        for handle in handles {
            let local_ids = handle.join().unwrap();
            for id in local_ids {
                assert!(
                    all_ids.insert(id),
                    "COLLISION DETECTED! Thread safety failed for ID: {}",
                    id
                );
            }
        }

        // 20 threads * 1,000 IDs = 20,000 unique IDs
        assert_eq!(all_ids.len(), 20_000);
    }

    #[test]
    fn test_machine_id_bitwise_extraction() {
        let expected_machine_id = 42;
        let generator = SnowflakeGenerator::new(expected_machine_id).unwrap();
        let id = generator.generate_id().unwrap();

        // Reverse-engineer the ID:
        // Shift right by 12 bits (to remove the sequence),
        // then apply a bitmask (1023) to isolate the 10-bit machine ID.
        let extracted_machine_id = (id >> 12) & 1023;

        assert_eq!(
            extracted_machine_id, expected_machine_id,
            "The embedded machine ID must match the initialized value"
        );
    }
}
