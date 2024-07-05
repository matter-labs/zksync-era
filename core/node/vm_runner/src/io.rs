use std::fmt::Debug;

use async_trait::async_trait;
use zksync_dal::{Connection, Core};
use zksync_types::L1BatchNumber;

/// Functionality to fetch/save data about processed/unprocessed batches for a particular VM runner
/// instance.
#[async_trait]
pub trait VmRunnerIo: Debug + Send + Sync + 'static {
    /// Unique name of the VM runner instance.
    fn name(&self) -> &'static str;

    /// Returns the last L1 batch number that has been processed by this VM runner instance.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    async fn latest_processed_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber>;

    /// Returns the last L1 batch number that is ready to be loaded by this VM runner instance.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    async fn last_ready_to_be_loaded_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber>;

    /// Marks the specified batch as the latest completed batch. All earlier batches are considered
    /// to be completed too. No guarantees about later batches.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    async fn mark_l1_batch_as_completed(
        &self,
        conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()>;
}
