//! Storage implementation based on DAL.

use zksync_concurrency::ctx;
use zksync_consensus_roles::validator;
use zksync_dal::consensus_dal;
use zksync_node_sync::{fetcher::FetchedBlock, sync_action::ActionQueueSender};
use zksync_state_keeper::io::common::FetcherCursor;

mod connection;
mod store;

pub(crate) use connection::*;
pub(crate) use store::*;

#[cfg(test)]
pub(crate) mod testonly;

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
    actions: ActionQueueSender,
}

impl PayloadQueue {
    /// converts the block into actions and pushes them to the actions queue.
    /// Does nothing and returns `Ok(false)` if the block has been already processed.
    /// Returns an error if a block with an earlier block number was expected.
    pub(crate) async fn send(
        &mut self,
        block: FetchedBlock,
        cursor: FetcherCursor,
    ) -> anyhow::Result<bool> {
        let want = cursor.next_l2_block;
        // Some blocks are missing.
        if block.number > want {
            anyhow::bail!("expected {want:?}, got {:?}", block.number);
        }
        // Block already processed.
        if block.number < want {
            return Ok(false);
        }
        self.actions
            .push_actions(block.into_actions(cursor))
            .await?;
        Ok(true)
    }
}
