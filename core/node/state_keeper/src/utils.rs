use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::watch;

pub(super) fn is_canceled(stop_receiver: &watch::Receiver<bool>) -> bool {
    *stop_receiver.borrow()
}

pub(super) fn seconds_since_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Incorrect system time")
        .as_secs()
}

pub(super) fn millis_since(since: u64) -> u64 {
    (seconds_since_epoch() - since) * 1000
}
