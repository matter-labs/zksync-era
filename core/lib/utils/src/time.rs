use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn seconds_since_epoch() -> u64 {
    duration_since_epoch().as_secs()
}

pub fn millis_since(since: u64) -> u64 {
    (millis_since_epoch() - since as u128 * 1000) as u64
}

pub fn millis_since_epoch() -> u128 {
    duration_since_epoch().as_millis()
}

fn duration_since_epoch() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Incorrect system time")
}
