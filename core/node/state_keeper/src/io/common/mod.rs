use std::time::Duration;

use anyhow::Context;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_types::{L1BatchNumber, L2BlockNumber, H256};
use zksync_vm_executor::storage::RestoredL1BatchEnv;

use super::PendingBatchData;

#[cfg(test)]
mod tests;

/// Returns the amount of iterations `delay_interval` fits into `max_wait`, rounding up.
pub(crate) fn poll_iters(delay_interval: Duration, max_wait: Duration) -> usize {
    let max_wait_millis = max_wait.as_millis() as u64;
    let delay_interval_millis = delay_interval.as_millis() as u64;
    assert!(delay_interval_millis > 0, "delay interval must be positive");

    max_wait_millis.div_ceil(delay_interval_millis).max(1) as usize
}

/// Cursor of the L2 block / L1 batch progress used by [`StateKeeperIO`](super::StateKeeperIO) implementations.
#[derive(Debug, Clone, Copy)]
pub struct IoCursor {
    pub next_l2_block: L2BlockNumber,
    pub prev_l2_block_hash: H256,
    pub prev_l2_block_timestamp: u64,
    pub l1_batch: L1BatchNumber,
    pub prev_l1_batch_timestamp: u64,
}

impl IoCursor {
    /// Loads the cursor from Postgres.
    pub async fn new(storage: &mut Connection<'_, Core>) -> anyhow::Result<Self> {
        let last_sealed_l1_batch_number_and_timestamp = storage
            .blocks_dal()
            .get_sealed_l1_batch_number_and_timestamp()
            .await?;
        let last_l2_block_header = storage
            .blocks_dal()
            .get_last_sealed_l2_block_header()
            .await?;

        if let (Some((l1_batch_number, l1_batch_timestamp)), Some(l2_block_header)) = (
            last_sealed_l1_batch_number_and_timestamp,
            &last_l2_block_header,
        ) {
            Ok(Self {
                next_l2_block: l2_block_header.number + 1,
                prev_l2_block_hash: l2_block_header.hash,
                prev_l2_block_timestamp: l2_block_header.timestamp,
                l1_batch: l1_batch_number + 1,
                prev_l1_batch_timestamp: l1_batch_timestamp,
            })
        } else {
            let snapshot_recovery = storage
                .snapshot_recovery_dal()
                .get_applied_snapshot_status()
                .await?
                .context("Postgres contains neither blocks nor snapshot recovery info")?;
            let (l1_batch, prev_l1_batch_timestamp) =
                if let Some((l1_batch_number, l1_batch_timestamp)) =
                    last_sealed_l1_batch_number_and_timestamp
                {
                    (l1_batch_number + 1, l1_batch_timestamp)
                } else {
                    (
                        snapshot_recovery.l1_batch_number + 1,
                        snapshot_recovery.l1_batch_timestamp,
                    )
                };

            let (next_l2_block, prev_l2_block_hash, prev_l2_block_timestamp);
            if let Some(l2_block_header) = &last_l2_block_header {
                next_l2_block = l2_block_header.number + 1;
                prev_l2_block_hash = l2_block_header.hash;
                prev_l2_block_timestamp = l2_block_header.timestamp;
            } else {
                next_l2_block = snapshot_recovery.l2_block_number + 1;
                prev_l2_block_hash = snapshot_recovery.l2_block_hash;
                prev_l2_block_timestamp = snapshot_recovery.l2_block_timestamp;
            }

            Ok(Self {
                next_l2_block,
                prev_l2_block_hash,
                prev_l2_block_timestamp,
                l1_batch,
                prev_l1_batch_timestamp,
            })
        }
    }
}

/// Loads the pending L1 batch data from the database.
///
/// # Errors
///
/// Propagates DB errors. Also returns an error if environment doesn't correspond to a pending L1 batch.
pub async fn load_pending_batch(
    storage: &mut Connection<'_, Core>,
    restored_l1_batch_env: RestoredL1BatchEnv,
) -> anyhow::Result<PendingBatchData> {
    let pending_l2_blocks = storage
        .transactions_dal()
        .get_l2_blocks_to_reexecute()
        .await?;
    let first_pending_l2_block = pending_l2_blocks
        .first()
        .context("no pending L2 blocks; was environment loaded for a correct L1 batch number?")?;
    let expected_pending_l2_block_number =
        L2BlockNumber(restored_l1_batch_env.l1_batch_env.first_l2_block.number);
    anyhow::ensure!(
        first_pending_l2_block.number == expected_pending_l2_block_number,
        "Invalid `L1BatchEnv` supplied: its L1 batch #{} is not pending; \
         first pending L2 block: {first_pending_l2_block:?}, first L2 block in batch: {:?}",
        restored_l1_batch_env.l1_batch_env.number,
        restored_l1_batch_env.l1_batch_env.first_l2_block
    );
    Ok(PendingBatchData {
        l1_batch_env: restored_l1_batch_env.l1_batch_env,
        system_env: restored_l1_batch_env.system_env,
        pubdata_params: restored_l1_batch_env.pubdata_params,
        pubdata_limit: restored_l1_batch_env.pubdata_limit,
        pending_l2_blocks,
    })
}
