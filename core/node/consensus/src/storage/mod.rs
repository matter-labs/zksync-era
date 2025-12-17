//! Storage implementation based on DAL.

use zksync_concurrency::ctx;
use zksync_consensus_roles::validator;
use zksync_dal::consensus_dal;
use zksync_node_sync::{
    fetcher::{FetchedBlock, IoCursorExt as _},
    sync_action::ActionQueueSender,
};
use zksync_state_keeper::io::common::IoCursor;

mod connection;
mod store;

pub(crate) use connection::*;
pub(crate) use store::*;
use zksync_shared_resources::api::SyncState;

#[cfg(test)]
pub(crate) mod testonly;

#[allow(clippy::large_enum_variant)]
#[derive(thiserror::Error, Debug)]
pub enum InsertCertificateError {
    #[error(transparent)]
    Canceled(#[from] ctx::Canceled),
    #[error(transparent)]
    Inner(#[from] consensus_dal::InsertCertificateError),
}

impl From<ctx::Error> for InsertCertificateError {
    fn from(err: ctx::Error) -> Self {
        match err {
            ctx::Error::Canceled(err) => Self::Canceled(err),
            ctx::Error::Internal(err) => Self::Inner(err.into()),
        }
    }
}

#[derive(Debug)]
pub(crate) struct PayloadQueue {
    inner: IoCursor,
    actions: ActionQueueSender,
    sync_state: SyncState,
}

impl PayloadQueue {
    pub(crate) fn next(&self) -> validator::BlockNumber {
        validator::BlockNumber(self.inner.next_l2_block.0.into())
    }

    /// Advances the cursor by converting the block into actions and pushing them
    /// to the actions queue.
    /// Does nothing and returns `Ok(())` if the block has been already processed.
    /// Returns an error if a block with an earlier block number was expected.
    pub(crate) async fn send(&mut self, block: FetchedBlock) -> anyhow::Result<()> {
        let want = self.inner.next_l2_block;
        // Some blocks are missing.
        if block.number > want {
            anyhow::bail!("expected {want:?}, got {:?}", block.number);
        }
        // Block already processed.
        if block.number < want {
            return Ok(());
        }
        self.actions.push_actions(self.inner.advance(block)).await?;
        Ok(())
    }
}
