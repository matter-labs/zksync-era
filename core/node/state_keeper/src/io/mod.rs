use std::{fmt, time::Duration};

use async_trait::async_trait;
use zksync_contracts::BaseSystemContracts;
use zksync_multivm::interface::{L1BatchEnv, SystemEnv};
use zksync_types::{
    block::L2BlockExecutionData, commitment::PubdataParams, fee_model::BatchFeeInput,
    protocol_upgrade::ProtocolUpgradeTx, Address, InteropRoot, L1BatchNumber, L2ChainId,
    ProtocolVersionId, Transaction, H256,
};
use zksync_vm_executor::storage::l1_batch_params;

pub use self::{
    common::IoCursor,
    output_handler::{OutputHandler, StateKeeperOutputHandler},
    persistence::{L2BlockSealerTask, StateKeeperPersistence, TreeWritesPersistence},
};
use super::seal_criteria::{IoSealCriteria, UnexecutableReason};

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
    pub(crate) pubdata_params: PubdataParams,
    /// List of L2 blocks and corresponding transactions that were executed within batch.
    pub(crate) pending_l2_blocks: Vec<L2BlockExecutionData>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct L2BlockParams {
    /// The timestamp of the L2 block in ms.
    timestamp_ms: u64,
    /// The maximal number of virtual blocks that can be created within this L2 block.
    /// During the migration from displaying users `batch.number` to L2 block number in Q3 2023
    /// in order to make the process smoother for users, we temporarily display the virtual blocks for users.
    ///
    /// Virtual blocks start their number with batch number and will increase until they reach the L2 block number.
    /// Note that it is the *maximal* number of virtual blocks that can be created within this L2 block since
    /// once the virtual blocks' number reaches the L2 block number, they will never be allowed to exceed those, i.e.
    /// any "excess" created blocks will be ignored.
    virtual_blocks: u32,
    interop_roots: Vec<InteropRoot>,
}

impl L2BlockParams {
    pub fn new(timestamp_ms: u64) -> Self {
        Self {
            timestamp_ms,
            virtual_blocks: 1,
            interop_roots: vec![],
        }
    }

    pub fn new_raw(
        timestamp_ms: u64,
        virtual_blocks: u32,
        interop_roots: Vec<InteropRoot>,
    ) -> Self {
        Self {
            timestamp_ms,
            virtual_blocks,
            interop_roots,
        }
    }

    /// The timestamp of the L2 block in seconds.
    pub fn timestamp(&self) -> u64 {
        self.timestamp_ms / 1000
    }

    /// The timestamp of the L2 block in milliseconds.
    pub fn timestamp_ms(&self) -> u64 {
        self.timestamp_ms
    }

    /// Mutable reference for the timestamp of the L2 block in milliseconds.
    pub fn timestamp_ms_mut(&mut self) -> &mut u64 {
        &mut self.timestamp_ms
    }

    pub fn virtual_blocks(&self) -> u32 {
        self.virtual_blocks
    }

    pub fn interop_roots(&self) -> &[InteropRoot] {
        &self.interop_roots
    }

    pub fn set_interop_roots(&mut self, interop_roots: Vec<InteropRoot>) {
        self.interop_roots = interop_roots;
    }
}

/// Parameters for a new L1 batch returned by [`StateKeeperIO::wait_for_new_batch_params()`].
#[derive(Debug, Clone, PartialEq)]
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
    /// Params related to how the pubdata should be processed by the bootloader in the batch.
    pub pubdata_params: PubdataParams,
}

#[derive(Debug)]
pub(crate) struct BatchInitParams {
    pub system_env: SystemEnv,
    pub l1_batch_env: L1BatchEnv,
    pub pubdata_params: PubdataParams,
    pub timestamp_ms: u64,
}

impl L1BatchParams {
    pub(crate) fn into_init_params(
        self,
        chain_id: L2ChainId,
        contracts: BaseSystemContracts,
        cursor: &IoCursor,
        previous_batch_hash: H256,
    ) -> BatchInitParams {
        let (system_env, l1_batch_env) = l1_batch_params(
            cursor.l1_batch,
            self.operator_address,
            self.first_l2_block.timestamp(),
            previous_batch_hash,
            self.fee_input,
            cursor.next_l2_block,
            cursor.prev_l2_block_hash,
            contracts,
            self.validation_computational_gas_limit,
            self.protocol_version,
            self.first_l2_block.virtual_blocks,
            chain_id,
        );

        BatchInitParams {
            system_env,
            l1_batch_env,
            pubdata_params: self.pubdata_params,
            timestamp_ms: self.first_l2_block.timestamp_ms(),
        }
    }
}

/// Provides the interactive layer for the state keeper:
/// it's used to receive volatile parameters (such as batch parameters) and sequence transactions
/// providing L2 block and L1 batch boundaries for them.
///
/// Methods with `&self` receiver must be cancel-safe; i.e., they should not use interior mutability
/// to change the I/O state. Methods with `&mut self` receiver don't need to be cancel-safe.
///
/// All errors returned from this method are treated as unrecoverable.
#[async_trait]
pub trait StateKeeperIO: 'static + Send + Sync + fmt::Debug + IoSealCriteria {
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

    /// Update the next block params timestamp
    fn update_next_l2_block_timestamp(&mut self, block_timestamp: &mut u64);

    /// Blocks for up to `max_wait` until the next transaction is available for execution.
    /// Returns `None` if no transaction became available until the timeout.
    async fn wait_for_next_tx(
        &mut self,
        max_wait: Duration,
        l2_block_timestamp: u64,
    ) -> anyhow::Result<Option<Transaction>>;
    /// Marks the transaction as "not executed", so it can be retrieved from the IO again.
    async fn rollback(&mut self, tx: Transaction) -> anyhow::Result<()>;
    /// Marks the transaction as "rejected", e.g. one that is not correct and can't be executed.
    async fn reject(&mut self, tx: &Transaction, reason: UnexecutableReason) -> anyhow::Result<()>;

    /// Loads base system contracts with the specified version.
    async fn load_base_system_contracts(
        &self,
        protocol_version: ProtocolVersionId,
        cursor: &IoCursor,
    ) -> anyhow::Result<BaseSystemContracts>;
    /// Loads protocol version of the specified L1 batch, which is guaranteed to exist in the storage.
    async fn load_batch_version_id(
        &self,
        number: L1BatchNumber,
    ) -> anyhow::Result<ProtocolVersionId>;
    /// Loads protocol upgrade tx for given version.
    async fn load_upgrade_tx(
        &self,
        version_id: ProtocolVersionId,
    ) -> anyhow::Result<Option<ProtocolUpgradeTx>>;

    /// Loads state hash for the L1 batch with the specified number. The batch is guaranteed to be present
    /// in the storage.
    async fn load_batch_state_hash(&self, number: L1BatchNumber) -> anyhow::Result<H256>;
}
