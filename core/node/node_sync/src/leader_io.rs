use std::time::Duration;

use async_trait::async_trait;
use zksync_contracts::BaseSystemContracts;
use zksync_state_keeper::{
    io::{common::IoCursor, L1BatchParams, L2BlockParams, PendingBatchData, StateKeeperIO},
    seal_criteria::{IoSealCriteria, UnexecutableReason},
    MempoolIO, UpdatesManager,
};
use zksync_types::{
    protocol_upgrade::ProtocolUpgradeTx, L1BatchNumber, L2ChainId, ProtocolVersionId, Transaction,
    H256,
};

use super::ExternalIO;

/// [`LeaderIO`] is the IO implementation for the state keeper
/// that should be used for leaders (i.e. nodes that can propose blocks).
///
/// In the leader mode it behaves the way `MempoolIO` does, it gets new transactions from mempool,
/// creates new blocks, decides when to seal block and batch.
///
/// Otherwise, it receives a sequence of actions from the fetcher via the action queue and propagates it
/// into the state keeper.
#[derive(Debug)]
pub struct LeaderIO {
    mempool_io: MempoolIO,
    external_io: ExternalIO,
    is_active_leader: bool,
}

impl LeaderIO {
    pub fn new(mempool_io: MempoolIO, external_io: ExternalIO) -> Self {
        Self {
            mempool_io,
            external_io,
            is_active_leader: false,
        }
    }

    pub fn active_io_ref(&self) -> &dyn StateKeeperIO {
        if self.is_active_leader {
            &self.mempool_io
        } else {
            &self.external_io
        }
    }

    pub fn active_io_ref_mut(&mut self) -> &mut dyn StateKeeperIO {
        if self.is_active_leader {
            &mut self.mempool_io
        } else {
            &mut self.external_io
        }
    }
}

#[async_trait]
impl IoSealCriteria for LeaderIO {
    async fn should_seal_l1_batch_unconditionally(
        &mut self,
        manager: &UpdatesManager,
    ) -> anyhow::Result<bool> {
        self.active_io_ref_mut()
            .should_seal_l1_batch_unconditionally(manager)
            .await
    }

    fn should_seal_l2_block(&mut self, manager: &UpdatesManager) -> bool {
        self.active_io_ref_mut().should_seal_l2_block(manager)
    }
}

#[async_trait]
impl StateKeeperIO for LeaderIO {
    fn chain_id(&self) -> L2ChainId {
        // Note: impls are the same for mempool and external.
        self.mempool_io.chain_id()
    }

    async fn initialize(&mut self) -> anyhow::Result<(IoCursor, Option<PendingBatchData>)> {
        self.mempool_io.initialize().await?;
        self.external_io.initialize().await
    }

    async fn wait_for_new_batch_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<Option<L1BatchParams>> {
        self.active_io_ref_mut()
            .wait_for_new_batch_params(cursor, max_wait)
            .await
    }

    async fn wait_for_new_l2_block_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<Option<L2BlockParams>> {
        self.active_io_ref_mut()
            .wait_for_new_l2_block_params(cursor, max_wait)
            .await
    }

    fn update_next_l2_block_timestamp(&mut self, block_timestamp: &mut u64) {
        self.active_io_ref_mut()
            .update_next_l2_block_timestamp(block_timestamp)
    }

    async fn wait_for_next_tx(
        &mut self,
        max_wait: Duration,
        l2_block_timestamp: u64,
    ) -> anyhow::Result<Option<Transaction>> {
        self.active_io_ref_mut()
            .wait_for_next_tx(max_wait, l2_block_timestamp)
            .await
    }

    async fn load_base_system_contracts(
        &self,
        protocol_version: ProtocolVersionId,
        cursor: &IoCursor,
    ) -> anyhow::Result<BaseSystemContracts> {
        // Note: impls are the same for mempool and external.
        self.mempool_io
            .load_base_system_contracts(protocol_version, cursor)
            .await
    }

    async fn load_batch_version_id(
        &self,
        number: L1BatchNumber,
    ) -> anyhow::Result<ProtocolVersionId> {
        // Note: impls are the same for mempool and external.
        self.mempool_io.load_batch_version_id(number).await
    }

    async fn load_upgrade_tx(
        &self,
        version_id: ProtocolVersionId,
    ) -> anyhow::Result<Option<ProtocolUpgradeTx>> {
        self.active_io_ref().load_upgrade_tx(version_id).await
    }

    async fn load_batch_state_hash(&self, number: L1BatchNumber) -> anyhow::Result<H256> {
        // Note: impls are the same for mempool and external.
        self.mempool_io.load_batch_state_hash(number).await
    }

    // TODO: what non-leader should do? signal to consensus that block is invalid?
    async fn reject(&mut self, tx: &Transaction, reason: UnexecutableReason) -> anyhow::Result<()> {
        let io = self.active_io_ref_mut();
        io.reject(tx, reason).await
    }

    // TODO: what non-leader should do? signal to consensus that block is invalid?
    async fn rollback(&mut self, tx: Transaction) -> anyhow::Result<()> {
        let io = self.active_io_ref_mut();
        io.rollback(tx).await
    }

    async fn rollback_l2_block(&mut self, txs: Vec<Transaction>) -> anyhow::Result<()> {
        let io = self.active_io_ref_mut();
        io.rollback_l2_block(txs).await
    }

    async fn advance_nonces(&mut self, txs: Box<&mut (dyn Iterator<Item = &Transaction> + Send)>) {
        self.mempool_io.advance_nonces(txs).await;
    }

    fn set_is_active_leader(&mut self, value: bool) {
        self.is_active_leader = value;
    }
}
