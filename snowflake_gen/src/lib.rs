use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use std::thread;

// --- 1. Constants (The Blueprint) ---
// Custom Epoch: Jan 1, 2024, 00:00:00 UTC (in milliseconds)
const CUSTOM_EPOCH: u64 = 1_704_067_200_000;

const MACHINE_ID_BITS: u64 = 10;
const SEQUENCE_BITS: u64 = 12;

const MAX_MACHINE_ID: u64 = (1 << MACHINE_ID_BITS) - 1; // 1023
const MAX_SEQUENCE: u64 = (1 << SEQUENCE_BITS) - 1;     // 4095

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
struct SnowflakeState {
    last_timestamp: u64,
    sequence: u64,
}

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