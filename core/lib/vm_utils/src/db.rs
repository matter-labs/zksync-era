use std::time::{Duration, Instant};

use zksync_dal::StorageProcessor;
use zksync_types::{L1BatchNumber, H256};

pub(crate) async fn wait_for_prev_l1_batch_params(
    storage: &mut StorageProcessor<'_>,
    number: L1BatchNumber,
) -> (H256, u64) {
    if number == L1BatchNumber(0) {
        return (H256::default(), 0);
    }
    wait_for_l1_batch_params_unchecked(storage, number - 1).await
}

/// # Warning
///
/// If invoked for a `L1BatchNumber` of a non-existent l1 batch, will block current thread indefinitely.
async fn wait_for_l1_batch_params_unchecked(
    storage: &mut StorageProcessor<'_>,
    number: L1BatchNumber,
) -> (H256, u64) {
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
            return (root_hash, timestamp);
        }

        tokio::time::sleep(SAFE_STATE_ROOT_INTERVAL).await;
    }
}
