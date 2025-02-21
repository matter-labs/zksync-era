//! State keeper persistence logic.

use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core};

use crate::{
    io::{
        seal_logic::l2_block_seal_subtasks::L2BlockSealProcess, IoCursor, StateKeeperOutputHandler,
    },
    updates::UpdatesManager,
};

/// Canonical [`HandleStateKeeperOutput`] implementation that stores processed L2 blocks and L1 batches to Postgres.
#[derive(Debug)]
pub struct StateKeeperPersistence {
    pool: ConnectionPool<Core>,
    pre_insert_txs: bool,
}

impl StateKeeperPersistence {
    const SHUTDOWN_MSG: &'static str = "L2 block sealer unexpectedly shut down";

    pub async fn new(pool: ConnectionPool<Core>) -> anyhow::Result<Self> {
        let this = Self {
            pool,
            pre_insert_txs: false,
        };
        Ok(this)
    }

    pub fn with_tx_insertion(mut self) -> Self {
        self.pre_insert_txs = true;
        self
    }
}

#[async_trait]
impl StateKeeperOutputHandler for StateKeeperPersistence {
    async fn initialize(&mut self, cursor: &IoCursor) -> anyhow::Result<()> {
        let mut connection = self.pool.connection_tagged("state_keeper").await?;
        L2BlockSealProcess::clear_pending_l2_block(&mut connection, cursor.next_l2_block - 1).await
    }

    async fn handle_block(&mut self, updates_manager: Arc<UpdatesManager>) -> anyhow::Result<()> {
        let command = updates_manager.seal_l2_block_command(self.pre_insert_txs);
        command.seal(self.pool.clone()).await.with_context(|| {
            format!(
                "cannot persist l2 block #{}",
                updates_manager.l2_block.number
            )
        })?;

        let batch_number = updates_manager.l1_batch_number;
        updates_manager
            .seal_l1_batch(self.pool.clone())
            .await
            .with_context(|| format!("cannot persist L1 batch #{batch_number}"))?;
        Ok(())
    }
}
