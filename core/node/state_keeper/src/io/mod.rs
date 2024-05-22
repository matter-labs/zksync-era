use std::{fmt, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use multivm::interface::{L1BatchEnv, SystemEnv};
use vm_utils::storage::l1_batch_params;
use zksync_contracts::BaseSystemContracts;
use zksync_types::{
    block::L2BlockExecutionData, fee_model::BatchFeeInput, protocol_upgrade::ProtocolUpgradeTx,
    Address, L1BatchNumber, L2ChainId, ProtocolVersionId, Transaction, H256,
};

pub use self::{
    common::IoCursor,
    output_handler::{OutputHandler, StateKeeperOutputHandler},
    persistence::{L2BlockSealerTask, StateKeeperPersistence, TreeWritesPersistence},
};
use super::seal_criteria::IoSealCriteria;

pub mod common;
pub(crate) mod mempool;
mod output_handler;
mod persistence;
pub mod seal_logic;
#[cfg(test)]
mod tests;

/// Contains information about the un-synced execution state:
/// Batch data and transactions that were executed before and are marked as so in the DB,
/// but aren't a part of a sealed batch.
///
/// Upon a restart, we must re-execute the pending state to continue progressing from the
/// place where we stopped.
///
/// Invariant is that there may be not more than 1 pending batch, and it's always the latest batch.
#[derive(Debug)]
pub struct PendingBatchData {
    /// Data used to initialize the pending batch. We have to make sure that all the parameters
    /// (e.g. timestamp) are the same, so transaction would have the same result after re-execution.
    pub(crate) l1_batch_env: L1BatchEnv,
    pub(crate) system_env: SystemEnv,
    /// List of L2 blocks and corresponding transactions that were executed within batch.
    pub(crate) pending_l2_blocks: Vec<L2BlockExecutionData>,
}

#[derive(Debug, Copy, Clone, Default)]
pub struct L2BlockParams {
    /// The timestamp of the L2 block.
    pub timestamp: u64,
    /// The maximal number of virtual blocks that can be created within this L2 block.
    /// During the migration from displaying users `batch.number` to L2 block number in Q3 2023
    /// in order to make the process smoother for users, we temporarily display the virtual blocks for users.
    ///
    /// Virtual blocks start their number with batch number and will increase until they reach the L2 block number.
    /// Note that it is the *maximal* number of virtual blocks that can be created within this L2 block since
    /// once the virtual blocks' number reaches the L2 block number, they will never be allowed to exceed those, i.e.
    /// any "excess" created blocks will be ignored.
    pub virtual_blocks: u32,
}

/// Parameters for a new L1 batch returned by [`StateKeeperIO::wait_for_new_batch_params()`].
#[derive(Debug, Clone)]
pub struct L1BatchParams {
    /// Protocol version for the new L1 batch.
    pub protocol_version: ProtocolVersionId,
    /// Computational gas limit for the new L1 batch.
    pub validation_computational_gas_limit: u32,
    /// Operator address (aka fee address) for the new L1 batch.
    pub operator_address: Address,
    /// Fee parameters to be used in the new L1 batch.
    pub fee_input: BatchFeeInput,
    /// Parameters of the first L2 block in the batch.
    pub first_l2_block: L2BlockParams,
}

impl L1BatchParams {
    pub(crate) fn into_env(
        self,
        chain_id: L2ChainId,
        contracts: BaseSystemContracts,
        cursor: &IoCursor,
        previous_batch_hash: H256,
    ) -> (SystemEnv, L1BatchEnv) {
        l1_batch_params(
            cursor.l1_batch,
            self.operator_address,
            self.first_l2_block.timestamp,
            previous_batch_hash,
            self.fee_input,
            cursor.next_l2_block,
            cursor.prev_l2_block_hash,
            contracts,
            self.validation_computational_gas_limit,
            self.protocol_version,
            self.first_l2_block.virtual_blocks,
            chain_id,
        )
    }
}

/// Provides the interactive layer for the state keeper:
/// it's used to receive volatile parameters (such as batch parameters) and sequence transactions
/// providing L2 block and L1 batch boundaries for them.
///
/// All errors returned from this method are treated as unrecoverable.
#[async_trait]
pub trait StateKeeperIO: 'static + Send + fmt::Debug + IoSealCriteria {
    /// Returns the ID of the L2 chain. This ID is supposed to be static.
    fn chain_id(&self) -> L2ChainId;

    /// Returns the data on the batch that was not sealed before the server restart.
    /// See `PendingBatchData` doc-comment for details.
    async fn initialize(&mut self) -> anyhow::Result<(IoCursor, Option<PendingBatchData>)>;

    /// Blocks for up to `max_wait` until the parameters for the next L1 batch are available.
    /// Returns the data required to initialize the VM for the next batch.
    async fn wait_for_new_batch_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<Option<L1BatchParams>>;

    /// Blocks for up to `max_wait` until the parameters for the next L2 block are available.
    async fn wait_for_new_l2_block_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<Option<L2BlockParams>>;

    /// Blocks for up to `max_wait` until the next transaction is available for execution.
    /// Returns `None` if no transaction became available until the timeout.
    async fn wait_for_next_tx(&mut self, max_wait: Duration)
        -> anyhow::Result<Option<Transaction>>;
    /// Marks the transaction as "not executed", so it can be retrieved from the IO again.
    async fn rollback(&mut self, tx: Transaction) -> anyhow::Result<()>;
    /// Marks the transaction as "rejected", e.g. one that is not correct and can't be executed.
    async fn reject(&mut self, tx: &Transaction, error: &str) -> anyhow::Result<()>;

    /// Loads base system contracts with the specified version.
    async fn load_base_system_contracts(
        &mut self,
        protocol_version: ProtocolVersionId,
        cursor: &IoCursor,
    ) -> anyhow::Result<BaseSystemContracts>;
    /// Loads protocol version of the specified L1 batch, which is guaranteed to exist in the storage.
    async fn load_batch_version_id(
        &mut self,
        number: L1BatchNumber,
    ) -> anyhow::Result<ProtocolVersionId>;
    /// Loads protocol upgrade tx for given version.
    async fn load_upgrade_tx(
        &mut self,
        version_id: ProtocolVersionId,
    ) -> anyhow::Result<Option<ProtocolUpgradeTx>>;
    /// Loads state hash for the L1 batch with the specified number. The batch is guaranteed to be present
    /// in the storage.
    async fn load_batch_state_hash(&mut self, number: L1BatchNumber) -> anyhow::Result<H256>;
}

impl dyn StateKeeperIO {
    pub(super) async fn wait_for_new_batch_env(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<Option<(SystemEnv, L1BatchEnv)>> {
        let Some(params) = self.wait_for_new_batch_params(cursor, max_wait).await? else {
            return Ok(None);
        };
        let contracts = self
            .load_base_system_contracts(params.protocol_version, cursor)
            .await
            .with_context(|| {
                format!(
                    "failed loading system contracts for protocol version {:?}",
                    params.protocol_version
                )
            })?;
        let previous_batch_hash = self
            .load_batch_state_hash(cursor.l1_batch - 1)
            .await
            .context("cannot load state hash for previous L1 batch")?;
        Ok(Some(params.into_env(
            self.chain_id(),
            contracts,
            cursor,
            previous_batch_hash,
        )))
    }
}
