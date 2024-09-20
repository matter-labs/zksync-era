use std::{fmt::Debug, sync::Arc};

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

    /// Marks the specified batch as being in progress. Will be called at least once before a batch can be marked
    /// as completed; can be called multiple times in case of a crash. The order in which this method is called
    /// is not specified; i.e., it is **not** guaranteed to be called sequentially.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    async fn mark_l1_batch_as_processing(
        &self,
        conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()>;

    /// Marks the specified batch as the latest completed batch. All earlier batches are considered
    /// to be completed too. No guarantees about later batches. This method is guaranteed to be called
    /// with monotonically increasing batch numbers.
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

#[async_trait]
impl<T: VmRunnerIo> VmRunnerIo for Arc<T> {
    fn name(&self) -> &'static str {
        (**self).name()
    }

    async fn latest_processed_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        (**self).latest_processed_batch(conn).await
    }

    async fn last_ready_to_be_loaded_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        (**self).last_ready_to_be_loaded_batch(conn).await
    }

    async fn mark_l1_batch_as_processing(
        &self,
        conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        (**self)
            .mark_l1_batch_as_processing(conn, l1_batch_number)
            .await
    }

    async fn mark_l1_batch_as_completed(
        &self,
        conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        (**self)
            .mark_l1_batch_as_completed(conn, l1_batch_number)
            .await
    }
}
