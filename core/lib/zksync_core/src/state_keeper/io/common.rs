use std::time::{Duration, Instant};

use vm_utils::vm_env::VmEnvBuilder;
use zksync_dal::StorageProcessor;
use zksync_types::{Address, L1BatchNumber, L2ChainId, H256};

use super::PendingBatchData;

/// Returns the amount of iterations `delay_interval` fits into `max_wait`, rounding up.
pub(crate) fn poll_iters(delay_interval: Duration, max_wait: Duration) -> usize {
    let max_wait_millis = max_wait.as_millis() as u64;
    let delay_interval_millis = delay_interval.as_millis() as u64;
    assert!(delay_interval_millis > 0, "delay interval must be positive");

    ((max_wait_millis + delay_interval_millis - 1) / delay_interval_millis).max(1) as usize
}

/// Loads the pending L1 block data from the database.
pub(crate) async fn load_pending_batch(
    storage: &mut StorageProcessor<'_>,
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    validation_computational_gas_limit: u32,
    chain_id: L2ChainId,
) -> Option<PendingBatchData> {
    let vm_env = VmEnvBuilder::new(
        current_l1_batch_number,
        validation_computational_gas_limit,
        chain_id,
    )
    .override_fee_account(fee_account)
    .build_for_pending_pending_batch(storage)
    .await
    .ok()?;

    let pending_miniblocks = storage
        .transactions_dal()
        .get_miniblocks_to_reexecute()
        .await
        .unwrap();

    Some(PendingBatchData {
        l1_batch_env: vm_env.l1_batch_env,
        system_env: vm_env.system_env,
        pending_miniblocks,
    })
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[rustfmt::skip] // One-line formatting looks better here.
    fn test_poll_iters() {
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(0)), 1);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(100)), 1);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(101)), 2);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(200)), 2);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(201)), 3);
    }
}
