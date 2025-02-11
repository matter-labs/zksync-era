use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::watch;

pub(super) fn is_canceled(stop_receiver: &watch::Receiver<bool>) -> bool {
    *stop_receiver.borrow()
}

// TODO (SMA-1206): use seconds instead of milliseconds.
pub(super) fn millis_since_epoch() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Incorrect system time")
        .as_millis()
}

pub(super) fn millis_since(since: u64) -> u64 {
    (millis_since_epoch() - since as u128 * 1000) as u64
}
