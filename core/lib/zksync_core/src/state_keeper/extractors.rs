//! Pure functions that convert data as required by the state keeper.

use chrono::{DateTime, TimeZone, Utc};

use std::{
    convert::TryFrom,
    fmt,
    time::{Duration, Instant},
};

use zksync_dal::StorageProcessor;
use zksync_types::{L1BatchNumber, U256};
use zksync_utils::h256_to_u256;

/// Displays a Unix timestamp (seconds since epoch) in human-readable form. Useful for logging.
pub(super) fn display_timestamp(timestamp: u64) -> impl fmt::Display {
    enum DisplayedTimestamp {
        Parsed(DateTime<Utc>),
        Raw(u64),
    }

    impl fmt::Display for DisplayedTimestamp {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Parsed(timestamp) => fmt::Display::fmt(timestamp, formatter),
                Self::Raw(raw) => write!(formatter, "(raw: {raw})"),
            }
        }
    }

    let parsed = i64::try_from(timestamp).ok();
    let parsed = parsed.and_then(|ts| Utc.timestamp_opt(ts, 0).single());
    parsed.map_or(
        DisplayedTimestamp::Raw(timestamp),
        DisplayedTimestamp::Parsed,
    )
}

pub(crate) async fn wait_for_prev_l1_batch_params(
    storage: &mut StorageProcessor<'_>,
    number: L1BatchNumber,
) -> (U256, u64) {
    if number == L1BatchNumber(0) {
        return (U256::default(), 0);
    }
    wait_for_l1_batch_params_unchecked(storage, number - 1).await
}

/// # Warning
///
/// If invoked for a `L1BatchNumber` of a non-existent l1 batch, will block current thread indefinitely.
async fn wait_for_l1_batch_params_unchecked(
    storage: &mut StorageProcessor<'_>,
    number: L1BatchNumber,
) -> (U256, u64) {
    // If the state root is not known yet, this duration will be used to back off in the while loops
    const SAFE_STATE_ROOT_INTERVAL: Duration = Duration::from_millis(100);

    let stage_started_at: Instant = Instant::now();
    loop {
        let data = storage
            .blocks_dal()
            .get_l1_batch_state_root_and_timestamp(number)
            .await
            .unwrap();
        if let Some((root_hash, timestamp)) = data {
            tracing::trace!(
                "Waiting for hash of L1 batch #{number} took {:?}",
                stage_started_at.elapsed()
            );
            return (h256_to_u256(root_hash), timestamp);
        }

        tokio::time::sleep(SAFE_STATE_ROOT_INTERVAL).await;
    }
}
