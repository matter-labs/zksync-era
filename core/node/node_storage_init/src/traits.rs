use std::fmt;

use tokio::sync::watch;
use zksync_types::{L1BatchNumber, OrStopped};

/// An abstract storage initialization strategy.
#[async_trait::async_trait]
pub trait InitializeStorage: fmt::Debug + Send + Sync + 'static {
    /// Checks if the storage is already initialized.
    async fn is_initialized(&self) -> anyhow::Result<bool>;

    /// Initializes the storage.
    /// Implementors of this method may assume that they have unique access to the storage.
    async fn initialize_storage(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> Result<(), OrStopped>;
}

/// An abstract storage revert strategy.
/// This trait assumes that for any invalid state there exists a batch number to which the storage can be rolled back.
#[async_trait::async_trait]
pub trait RevertStorage: fmt::Debug + Send + Sync + 'static {
    /// Checks whether a reorg is needed for the storage.
    async fn is_reorg_needed(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> Result<bool, OrStopped>;

    /// Checks if the storage is invalid state and has to be rolled back.
    async fn last_correct_batch_for_reorg(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> Result<Option<L1BatchNumber>, OrStopped>;

    /// Reverts the storage to the provided batch number.
    async fn revert_storage(&self, to_batch: L1BatchNumber) -> anyhow::Result<()>;
}
