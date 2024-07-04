use std::fmt;

use tokio::sync::watch;
use zksync_types::L1BatchNumber;

/// An abstract storage initialization strategy.
#[async_trait::async_trait]
pub trait InitializeStorage: fmt::Debug + Send + Sync + 'static {
    /// Checks if the storage is already initialized.
    async fn is_initialized(&self) -> anyhow::Result<bool>;

    /// Initializes the storage.
    /// Implementors of this method may assume that they have unique access to the storage.
    async fn initialize_storage(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()>;
}

/// An abstract storage revert strategy.
/// This trait assumes that for any invalid state there exists a batch number to which the storage can be rolled back.
#[async_trait::async_trait]
pub trait RevertStorage: fmt::Debug + Send + Sync + 'static {
    /// Checks if the storage is invalid state and has to be rolled back.
    async fn last_correct_batch_for_reorg(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<Option<L1BatchNumber>>;

    /// Reverts the storage to the provided batch number.
    async fn revert_storage(
        &self,
        to_batch: L1BatchNumber,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()>;
}
