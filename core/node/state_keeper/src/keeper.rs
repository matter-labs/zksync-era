use std::{collections::VecDeque, convert::Infallible, sync::Arc, time::Duration};

use anyhow::Context as _;
use tokio::sync::watch;
use tracing::{info_span, Instrument};
use zksync_health_check::{HealthUpdater, ReactiveHealthCheck};
use zksync_multivm::{
    interface::{
        executor::{BatchExecutor, BatchExecutorFactory},
        Halt, L1BatchEnv, SystemEnv,
    },
    utils::StorageWritesDeduplicator,
};
use zksync_shared_metrics::{TxStage, APP_METRICS};
use zksync_state::{OwnedStorage, ReadStorageFactory};
use zksync_types::{
    block::L2BlockExecutionData, commitment::PubdataParams, l2::TransactionType,
    protocol_upgrade::ProtocolUpgradeTx, protocol_version::ProtocolVersionId, try_stoppable,
    utils::display_timestamp, L1BatchNumber, L2BlockNumber, OrStopped, StopContext, Transaction,
};
use zksync_vm_executor::whitelist::DeploymentTxFilter;

use crate::{
    executor::TxExecutionResult,
    health::StateKeeperHealthDetails,
    io::{BatchInitParams, IoCursor, L1BatchParams, L2BlockParams, OutputHandler, StateKeeperIO},
    metrics::{AGGREGATION_METRICS, KEEPER_METRICS},
    seal_criteria::{ConditionalSealer, SealData, SealResolution, UnexecutableReason},
    updates::UpdatesManager,
    utils::is_canceled,
};

/// Amount of time to block on waiting for some resource. The exact value is not really important,
/// we only need it to not block on waiting indefinitely and be able to process cancellation requests.
pub(super) const POLL_WAIT_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug)]
struct InitializedBatchState {
    updates_manager: UpdatesManager,
    batch_executor: Box<dyn BatchExecutor<OwnedStorage>>,
    protocol_upgrade_tx: Option<ProtocolUpgradeTx>,
    next_block_should_be_fictive: bool,
}

#[derive(Debug)]
enum BatchState {
    Uninit(IoCursor),
    Init(Box<InitializedBatchState>),
}

impl BatchState {
    /// Initializes batch if it was not.
    /// Returns mutable reference of the initialized state.
    async fn ensure_initialized(
        &mut self,
        inner: &mut StateKeeperInner,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> Result<&mut InitializedBatchState, OrStopped> {
        let state = match self {
            Self::Uninit(cursor) => inner.start_batch(*cursor, stop_receiver).await?,
            Self::Init(init) => return Ok(init.as_mut()),
        };
        *self = Self::Init(Box::new(state));
        match self {
            Self::Init(init) => Ok(init.as_mut()),
            Self::Uninit(_) => unreachable!(),
        }
    }

    /// Changes the state to `Uninit` with the cursor for the next batch.
    /// Returns the old state.
    fn finish(&mut self) -> Box<InitializedBatchState> {
        let next_cursor = match self {
            Self::Uninit(_) => panic!("Unexpected `BatchState::Uninit`"),
            Self::Init(init) => {
                let mut cursor = init.updates_manager.io_cursor();
                cursor.l1_batch += 1;
                cursor.prev_l1_batch_timestamp = init.updates_manager.l1_batch_timestamp();
                cursor
            }
        };
        let state = std::mem::replace(self, Self::Uninit(next_cursor));
        match state {
            Self::Uninit(_) => unreachable!(),
            Self::Init(init) => init,
        }
    }

    fn unwrap_init_ref(&self) -> &InitializedBatchState {
        match self {
            Self::Uninit(_) => panic!("Unexpected `BatchState::Uninit`"),
            Self::Init(init) => init.as_ref(),
        }
    }

    fn unwrap_init_mut(&mut self) -> &mut InitializedBatchState {
        match self {
            Self::Uninit(_) => panic!("Unexpected `BatchState::Uninit`"),
            Self::Init(init) => init.as_mut(),
        }
    }
}

/// State keeper represents a logic layer of L1 batch / L2 block processing flow.
///
/// It's responsible for taking all the data from the `StateKeeperIO`, feeding it into `BatchExecutor` objects
/// and calling `SealManager` to decide whether an L2 block or L1 batch should be sealed.
///
/// State keeper maintains the batch execution state in the `UpdatesManager` until batch is sealed and these changes
/// are persisted by the `StateKeeperIO` implementation.
///
/// You can think of it as a state machine that runs over a sequence of incoming transactions, turning them into
/// a sequence of executed L2 blocks and batches.
#[derive(Debug)]
pub struct StateKeeper {
    inner: StateKeeperInner,
    batch_state: BatchState,
}

#[derive(Debug)]
pub struct StateKeeperBuilder {
    io: Box<dyn StateKeeperIO>,
    output_handler: OutputHandler,
    batch_executor_factory: Box<dyn BatchExecutorFactory<OwnedStorage>>,
    sealer: Arc<dyn ConditionalSealer>,
    storage_factory: Arc<dyn ReadStorageFactory>,
    health_updater: HealthUpdater,
    deployment_tx_filter: Option<DeploymentTxFilter>,
    leader_rotation: bool,
}

/// Helper struct that encapsulates some private state keeper methods.
#[derive(Debug)]
pub(super) struct StateKeeperInner {
    io: Box<dyn StateKeeperIO>,
    output_handler: OutputHandler,
    batch_executor_factory: Box<dyn BatchExecutorFactory<OwnedStorage>>,
    sealer: Arc<dyn ConditionalSealer>,
    storage_factory: Arc<dyn ReadStorageFactory>,
    health_updater: HealthUpdater,
    deployment_tx_filter: Option<DeploymentTxFilter>,
    leader_rotation: bool,
}

impl From<StateKeeperBuilder> for StateKeeperInner {
    fn from(b: StateKeeperBuilder) -> Self {
        Self {
            io: b.io,
            output_handler: b.output_handler,
            batch_executor_factory: b.batch_executor_factory,
            sealer: b.sealer,
            storage_factory: b.storage_factory,
            health_updater: b.health_updater,
            deployment_tx_filter: b.deployment_tx_filter,
            leader_rotation: b.leader_rotation,
        }
    }
}

impl StateKeeperBuilder {
    pub fn new(
        sequencer: Box<dyn StateKeeperIO>,
        batch_executor_factory: Box<dyn BatchExecutorFactory<OwnedStorage>>,
        output_handler: OutputHandler,
        sealer: Arc<dyn ConditionalSealer>,
        storage_factory: Arc<dyn ReadStorageFactory>,
        deployment_tx_filter: Option<DeploymentTxFilter>,
    ) -> Self {
        Self {
            io: sequencer,
            batch_executor_factory,
            output_handler,
            sealer,
            storage_factory,
            health_updater: ReactiveHealthCheck::new("state_keeper").1,
            deployment_tx_filter,
            leader_rotation: false,
        }
    }

    pub fn with_leader_rotation(mut self, sync: bool) -> Self {
        self.leader_rotation = sync;
        self
    }

    pub async fn build(
        self,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<StateKeeper, OrStopped> {
        let mut inner = StateKeeperInner::from(self);
        let (cursor, pending_batch_params) = inner.io.initialize().await?;
        inner.output_handler.initialize(&cursor).await?;
        inner
            .health_updater
            .update(StateKeeperHealthDetails::from(&cursor).into());
        tracing::info!(
            "Initializing state keeper. Next l1 batch to seal: {}, next L2 block to seal: {}",
            cursor.l1_batch,
            cursor.next_l2_block
        );

        // Re-execute pending batch if it exists. Otherwise, initialize a new batch.
        let (batch_init_params, pending_l2_blocks) = match pending_batch_params {
            Some(params) => {
                tracing::info!(
                    "There exists a pending batch consisting of {} L2 blocks, the first one is {}",
                    params.pending_l2_blocks.len(),
                    params
                        .pending_l2_blocks
                        .first()
                        .context("expected at least one pending L2 block")?
                        .number
                );
                // For re-executing purposes it's ok to not use exact precise millis.
                let timestamp_ms = params.l1_batch_env.first_l2_block.timestamp * 1000;
                (
                    BatchInitParams {
                        system_env: params.system_env,
                        l1_batch_env: params.l1_batch_env,
                        pubdata_params: params.pubdata_params,
                        timestamp_ms,
                    },
                    params.pending_l2_blocks,
                )
            }
            None => {
                tracing::info!("There is no open pending batch");
                return Ok(StateKeeper {
                    inner,
                    batch_state: BatchState::Uninit(cursor),
                });
            }
        };

        let previous_batch_protocol_version = inner
            .io
            .load_batch_version_id(batch_init_params.l1_batch_env.number - 1)
            .await?;
        let mut updates_manager = UpdatesManager::new(
            &batch_init_params,
            previous_batch_protocol_version,
            cursor.prev_l1_batch_timestamp,
            None,
            !inner.leader_rotation,
        );
        let protocol_upgrade_tx: Option<ProtocolUpgradeTx> = inner
            .load_protocol_upgrade_tx(
                &pending_l2_blocks,
                batch_init_params.system_env.version,
                batch_init_params.l1_batch_env.number,
            )
            .await?;

        let mut batch_executor = inner
            .create_batch_executor(
                batch_init_params.l1_batch_env.clone(),
                batch_init_params.system_env.clone(),
                batch_init_params.pubdata_params,
                stop_receiver,
            )
            .await?;
        StateKeeperInner::restore_state(
            &mut *batch_executor,
            &mut updates_manager,
            pending_l2_blocks,
        )
        .await?;

        Ok(StateKeeper {
            inner,
            batch_state: BatchState::Init(Box::new(InitializedBatchState {
                updates_manager,
                batch_executor,
                protocol_upgrade_tx,
                next_block_should_be_fictive: false,
            })),
        })
    }

    /// Returns the health check for state keeper.
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }
}

impl StateKeeperInner {
    async fn start_batch(
        &mut self,
        cursor: IoCursor,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> Result<InitializedBatchState, OrStopped> {
        let batch_init_params = self
            .wait_for_new_batch_init_params(&cursor, stop_receiver)
            .await?;
        let first_batch_in_shared_bridge = batch_init_params.l1_batch_env.number
            == L1BatchNumber(1)
            && !batch_init_params.system_env.version.is_pre_shared_bridge();
        let previous_batch_protocol_version = self
            .io
            .load_batch_version_id(batch_init_params.l1_batch_env.number - 1)
            .await?;
        let updates_manager = UpdatesManager::new(
            &batch_init_params,
            previous_batch_protocol_version,
            cursor.prev_l1_batch_timestamp,
            Some(cursor.prev_l2_block_timestamp),
            !self.leader_rotation,
        );
        let batch_executor = self
            .create_batch_executor(
                batch_init_params.l1_batch_env.clone(),
                batch_init_params.system_env.clone(),
                batch_init_params.pubdata_params,
                stop_receiver,
            )
            .await?;

        let version_changed =
            batch_init_params.system_env.version != previous_batch_protocol_version;
        let protocol_upgrade_tx = if version_changed || first_batch_in_shared_bridge {
            self.load_upgrade_tx(batch_init_params.system_env.version)
                .await?
        } else {
            None
        };

        Ok(InitializedBatchState {
            updates_manager,
            batch_executor,
            protocol_upgrade_tx,
            next_block_should_be_fictive: false,
        })
    }

    async fn create_batch_executor(
        &mut self,
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        pubdata_params: PubdataParams,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<Box<dyn BatchExecutor<OwnedStorage>>, OrStopped> {
        let storage = self
            .storage_factory
            .access_storage(stop_receiver, l1_batch_env.number - 1)
            .await
            .stop_context("failed creating VM storage")?;
        Ok(self.batch_executor_factory.init_batch(
            storage,
            l1_batch_env,
            system_env,
            pubdata_params,
        ))
    }

    /// This function is meant to be called only once during the state-keeper initialization.
    /// It will check if we should load a protocol upgrade or a `GenesisUpgrade` transaction,
    /// perform some checks and return it.
    pub(super) async fn load_protocol_upgrade_tx(
        &mut self,
        pending_l2_blocks: &[L2BlockExecutionData],
        protocol_version: ProtocolVersionId,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<ProtocolUpgradeTx>> {
        // After the Shared Bridge is integrated,
        // there has to be a GenesisUpgrade upgrade transaction after the chain genesis.
        // It has to be the first transaction of the first batch.
        // The GenesisUpgrade upgrade does not bump the protocol version, but attaches an upgrade
        // transaction to the genesis protocol version.
        let first_batch_in_shared_bridge =
            l1_batch_number == L1BatchNumber(1) && !protocol_version.is_pre_shared_bridge();
        let previous_batch_protocol_version =
            self.io.load_batch_version_id(l1_batch_number - 1).await?;

        let version_changed = protocol_version != previous_batch_protocol_version;
        let mut protocol_upgrade_tx = if version_changed || first_batch_in_shared_bridge {
            self.io.load_upgrade_tx(protocol_version).await?
        } else {
            None
        };

        // Sanity check: if `txs_to_reexecute` is not empty and upgrade tx is present for this block
        // then it must be the first one in `txs_to_reexecute`.
        if !pending_l2_blocks.is_empty() && protocol_upgrade_tx.is_some() {
            // We already processed the upgrade tx but did not seal the batch it was in.
            let first_tx_to_reexecute = &pending_l2_blocks[0].txs[0];
            assert_eq!(
                first_tx_to_reexecute.tx_format(),
                TransactionType::ProtocolUpgradeTransaction,
                "Expected an upgrade transaction to be the first one in pending L2 blocks, but found {:?}",
                first_tx_to_reexecute.hash()
            );
            tracing::info!(
                "There is a protocol upgrade in batch #{l1_batch_number}, upgrade tx already processed"
            );
            protocol_upgrade_tx = None; // The protocol upgrade was already executed
        }

        if protocol_upgrade_tx.is_some() {
            tracing::info!("There is a new upgrade tx to be executed in batch #{l1_batch_number}");
        }
        Ok(protocol_upgrade_tx)
    }

    async fn load_upgrade_tx(
        &mut self,
        protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<Option<ProtocolUpgradeTx>> {
        self.io
            .load_upgrade_tx(protocol_version)
            .await
            .with_context(|| format!("failed loading upgrade transaction for {protocol_version:?}"))
    }

    #[tracing::instrument(
        skip_all,
        fields(
            l1_batch = %cursor.l1_batch,
        )
    )]
    async fn wait_for_new_batch_params(
        &mut self,
        cursor: &IoCursor,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<L1BatchParams, OrStopped> {
        while !is_canceled(stop_receiver) {
            if let Some(params) = self
                .io
                .wait_for_new_batch_params(cursor, POLL_WAIT_DURATION)
                .await?
            {
                return Ok(params);
            }
        }
        Err(OrStopped::Stopped)
    }

    #[tracing::instrument(
        skip_all,
        fields(
            l1_batch = %cursor.l1_batch,
        )
    )]
    async fn wait_for_new_batch_init_params(
        &mut self,
        cursor: &IoCursor,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> Result<BatchInitParams, OrStopped> {
        // `io.wait_for_new_batch_params(..)` is not cancel-safe; once we get new batch params, we must hold onto them
        // until we get the rest of parameters from I/O or receive a stop request.
        let params = self
            .wait_for_new_batch_params(cursor, stop_receiver)
            .await?;
        let contracts = self
            .io
            .load_base_system_contracts(params.protocol_version, cursor)
            .await
            .with_context(|| {
                format!(
                    "failed loading system contracts for protocol version {:?}",
                    params.protocol_version
                )
            })?;

        // `select!` is safe to use here; `io.load_batch_state_hash(..)` is cancel-safe by contract
        tokio::select! {
            hash_result = self.io.load_batch_state_hash(cursor.l1_batch - 1) => {
                let previous_batch_hash = hash_result.context("cannot load state hash for previous L1 batch")?;
                Ok(params.into_init_params(self.io.chain_id(), contracts, cursor, previous_batch_hash))
            }
            _ = stop_receiver.changed() => Err(OrStopped::Stopped),
        }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            l1_batch = %updates.l1_batch_number(),
            l2_block = %updates.next_l2_block_number(),
        )
    )]
    async fn wait_for_new_l2_block_params(
        &mut self,
        updates: &UpdatesManager,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<L2BlockParams, OrStopped> {
        let latency = KEEPER_METRICS.wait_for_l2_block_params.start();
        let cursor = updates.io_cursor();
        while !is_canceled(stop_receiver) {
            if let Some(params) = self
                .io
                .wait_for_new_l2_block_params(&cursor, POLL_WAIT_DURATION)
                .await
                .context("error waiting for new L2 block params")?
            {
                self.health_updater
                    .update(StateKeeperHealthDetails::from(&cursor).into());

                latency.observe();
                return Ok(params);
            }
        }
        Err(OrStopped::Stopped)
    }

    fn set_l2_block_params(updates_manager: &mut UpdatesManager, l2_block_params: L2BlockParams) {
        tracing::debug!(
            "Setting next L2 block #{} (L1 batch #{}) with initial params: timestamp {}, virtual block {}",
            updates_manager.next_l2_block_number(),
            updates_manager.l1_batch_number(),
            display_timestamp(l2_block_params.timestamp()),
            l2_block_params.virtual_blocks()
        );
        updates_manager.set_next_l2_block_params(l2_block_params);
    }

    #[tracing::instrument(
        skip_all,
        fields(
            l1_batch = %updates_manager.l1_batch_number(),
            l2_block = %updates_manager.next_l2_block_number(),
        )
    )]
    async fn start_next_l2_block(
        updates_manager: &mut UpdatesManager,
        batch_executor: &mut dyn BatchExecutor<OwnedStorage>,
    ) -> anyhow::Result<()> {
        updates_manager.push_l2_block();
        let block_env = updates_manager.last_pending_l2_block().get_env();
        tracing::debug!(
            "Initialized new L2 block #{} (L1 batch #{}) with timestamp {}",
            block_env.number,
            updates_manager.l1_batch_number(),
            display_timestamp(block_env.timestamp)
        );
        batch_executor
            .start_next_l2_block(block_env)
            .await
            .with_context(|| {
                format!("failed starting L2 block with {block_env:?} in batch executor")
            })
    }

    #[tracing::instrument(
        skip_all,
        fields(
            l1_batch = %updates_manager.l1_batch_number(),
            l2_block = %updates_manager.last_pending_l2_block().number,
        )
    )]
    async fn seal_l2_block(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        self.output_handler
            .handle_l2_block_data(updates_manager)
            .await
            .with_context(|| {
                format!(
                    "handling L2 block #{} failed",
                    updates_manager.last_pending_l2_block().number
                )
            })
    }

    /// Applies the "pending state" on the `UpdatesManager`.
    /// Pending state means transactions that were executed before the server restart. Before we continue processing the
    /// batch, we need to restore the state. We must ensure that every transaction is executed successfully.
    ///
    /// Additionally, it initialized the next L2 block timestamp.
    #[tracing::instrument(
        skip_all,
        fields(n_blocks = %l2_blocks_to_reexecute.len())
    )]
    async fn restore_state(
        batch_executor: &mut dyn BatchExecutor<OwnedStorage>,
        updates_manager: &mut UpdatesManager,
        l2_blocks_to_reexecute: Vec<L2BlockExecutionData>,
    ) -> Result<(), OrStopped> {
        if l2_blocks_to_reexecute.is_empty() {
            return Ok(());
        }

        for (index, l2_block) in l2_blocks_to_reexecute.into_iter().enumerate() {
            // Push any non-first L2 block to updates manager. The first one was pushed when `updates_manager` was initialized.
            if index > 0 {
                Self::set_l2_block_params(
                    updates_manager,
                    // For re-executing purposes it's ok to not use exact precise millis.
                    L2BlockParams::with_custom_virtual_block_count(
                        l2_block.timestamp * 1000,
                        l2_block.virtual_blocks,
                    ),
                );
                Self::start_next_l2_block(updates_manager, batch_executor).await?;
            }

            let l2_block_number = l2_block.number;
            tracing::info!(
                "Starting to reexecute transactions from sealed L2 block #{l2_block_number}"
            );
            for tx in l2_block.txs {
                let result = batch_executor
                    .execute_tx(tx.clone())
                    .await
                    .with_context(|| format!("failed re-executing transaction {:?}", tx.hash()))?;
                let result = TxExecutionResult::new(result);

                APP_METRICS.processed_txs[&TxStage::StateKeeper].inc();
                APP_METRICS.processed_l1_txs[&TxStage::StateKeeper].inc_by(tx.is_l1().into());

                let TxExecutionResult::Success {
                    tx_result,
                    tx_metrics: tx_execution_metrics,
                    call_tracer_result,
                    ..
                } = result
                else {
                    tracing::error!(
                        "Re-executing stored tx failed. Tx: {tx:?}. Err: {:?}",
                        result.err()
                    );
                    return Err(anyhow::anyhow!(
                        "Re-executing stored tx failed. It means that transaction was executed \
                         successfully before, but failed after a restart."
                    )
                    .into());
                };

                let tx_hash = tx.hash();
                let is_l1 = tx.is_l1();
                let exec_result_status = tx_result.result.clone();
                let initiator_account = tx.initiator_account();

                updates_manager.extend_from_executed_transaction(
                    tx,
                    *tx_result,
                    *tx_execution_metrics,
                    call_tracer_result,
                );

                tracing::debug!(
                    "Finished re-executing tx {tx_hash} by {initiator_account} (is_l1: {is_l1}, \
                     #{idx_in_l1_batch} in L1 batch #{l1_batch_number}, #{idx_in_l2_block} in L2 block #{l2_block_number}); \
                     status: {exec_result_status:?}. Tx execution metrics: {tx_execution_metrics:?}, block execution metrics: {block_execution_metrics:?}",
                    idx_in_l1_batch = updates_manager.pending_executed_transactions_len(),
                    l1_batch_number = updates_manager.l1_batch_number(),
                    idx_in_l2_block = updates_manager.last_pending_l2_block().executed_transactions.len(),
                    block_execution_metrics = updates_manager.pending_execution_metrics()
                );
            }

            updates_manager.commit_pending_block();
        }

        tracing::debug!(
            "All the transactions from the pending state were re-executed successfully"
        );
        Ok(())
    }

    async fn process_upgrade_tx(
        &mut self,
        batch_executor: &mut dyn BatchExecutor<OwnedStorage>,
        updates_manager: &mut UpdatesManager,
        protocol_upgrade_tx: ProtocolUpgradeTx,
    ) -> anyhow::Result<()> {
        // Sanity check: protocol upgrade tx must be the first one in the batch.
        assert_eq!(updates_manager.pending_executed_transactions_len(), 0);

        let tx: Transaction = protocol_upgrade_tx.into();
        let (seal_resolution, exec_result) = self
            .process_one_tx(batch_executor, updates_manager, tx.clone())
            .await?;

        match &seal_resolution {
            SealResolution::NoSeal | SealResolution::IncludeAndSeal => {
                let TxExecutionResult::Success {
                    tx_result,
                    tx_metrics: tx_execution_metrics,
                    call_tracer_result,
                    ..
                } = exec_result
                else {
                    anyhow::bail!("Tx inclusion seal resolution must be a result of a successful tx execution");
                };

                // Despite success of upgrade transaction is not enforced by protocol,
                // we panic here because failed upgrade tx is not intended in any case.
                if tx_result.result.is_failed() {
                    anyhow::bail!("Failed upgrade tx {:?}", tx.hash());
                }

                updates_manager.extend_from_executed_transaction(
                    tx,
                    *tx_result,
                    *tx_execution_metrics,
                    call_tracer_result,
                );
                Ok(())
            }
            SealResolution::ExcludeAndSeal => {
                anyhow::bail!("first tx in batch cannot result into `ExcludeAndSeal`");
            }
            SealResolution::Unexecutable(reason) => {
                anyhow::bail!(
                    "Upgrade transaction {:?} is unexecutable: {reason}",
                    tx.hash()
                );
            }
        }
    }

    /// Executes one transaction in the batch executor, and then decides whether the batch should be sealed.
    /// Batch may be sealed because of one of the following reasons:
    /// 1. The VM entered an incorrect state (e.g. out of gas). In that case, we must revert the transaction and seal
    /// the block.
    /// 2. Seal manager decided that batch is ready to be sealed.
    /// Note: this method doesn't mutate `updates_manager` in the end. However, reference should be mutable
    /// because we use `apply_and_rollback` method of `updates_manager.storage_writes_deduplicator`.
    #[tracing::instrument(skip_all)]
    async fn process_one_tx(
        &mut self,
        batch_executor: &mut dyn BatchExecutor<OwnedStorage>,
        updates_manager: &mut UpdatesManager,
        tx: Transaction,
    ) -> anyhow::Result<(SealResolution, TxExecutionResult)> {
        let latency = KEEPER_METRICS.execute_tx_outer_time.start();
        let exec_result = batch_executor
            .execute_tx(tx.clone())
            .await
            .with_context(|| format!("failed executing transaction {:?}", tx.hash()))?;
        let exec_result = TxExecutionResult::new(exec_result);
        latency.observe();

        APP_METRICS.processed_txs[&TxStage::StateKeeper].inc();
        APP_METRICS.processed_l1_txs[&TxStage::StateKeeper].inc_by(tx.is_l1().into());

        let latency = KEEPER_METRICS.determine_seal_resolution.start();
        // All of `TxExecutionResult::BootloaderOutOfGasForTx`,
        // `Halt::NotEnoughGasProvided` correspond to out-of-gas errors but of different nature.
        // - `BootloaderOutOfGasForTx`: it is returned when bootloader stack frame run out of gas before tx execution finished.
        // - `Halt::NotEnoughGasProvided`: there are checks in bootloader in some places (search for `checkEnoughGas` calls).
        //      They check if there is more gas in the frame than bootloader estimates it will need.
        //      This error is returned when such a check fails. Basically, bootloader doesn't continue execution but panics prematurely instead.
        // If some transaction fails with any of these errors and is the first transaction in L1 batch, then it's marked as unexecutable.
        // Otherwise, `ExcludeAndSeal` resolution is returned, i.e. batch will be sealed and transaction will be included in the next L1 batch.

        let is_first_tx = updates_manager.pending_executed_transactions_len() == 0;
        let resolution = match &exec_result {
            TxExecutionResult::BootloaderOutOfGasForTx
            | TxExecutionResult::RejectedByVm {
                reason: Halt::NotEnoughGasProvided,
            } => {
                let (reason, criterion) = match &exec_result {
                    TxExecutionResult::BootloaderOutOfGasForTx => (
                        UnexecutableReason::BootloaderOutOfGas,
                        "bootloader_tx_out_of_gas",
                    ),
                    TxExecutionResult::RejectedByVm {
                        reason: Halt::NotEnoughGasProvided,
                    } => (
                        UnexecutableReason::NotEnoughGasProvided,
                        "not_enough_gas_provided_to_start_tx",
                    ),
                    _ => unreachable!(),
                };
                let resolution = if is_first_tx {
                    SealResolution::Unexecutable(reason)
                } else {
                    SealResolution::ExcludeAndSeal
                };
                AGGREGATION_METRICS.l1_batch_reason_inc(criterion, &resolution);
                resolution
            }
            TxExecutionResult::RejectedByVm { reason } => {
                UnexecutableReason::Halt(reason.clone()).into()
            }
            TxExecutionResult::Success {
                tx_result,
                tx_metrics: tx_execution_metrics,
                gas_remaining,
                ..
            } => {
                let tx_execution_status = &tx_result.result;
                tracing::trace!(
                    "finished tx {:?} by {:?} (is_l1: {}) (#{} in l1 batch {}) (#{} in L2 block {}) \
                    status: {:?}. Tx execution metrics: {:?}, block execution metrics: {:?}",
                    tx.hash(),
                    tx.initiator_account(),
                    tx.is_l1(),
                    updates_manager.pending_executed_transactions_len() + 1,
                    updates_manager.l1_batch_number(),
                    updates_manager.last_pending_l2_block().executed_transactions.len() + 1,
                    updates_manager.last_pending_l2_block().number,
                    tx_execution_status,
                    &tx_execution_metrics,
                    updates_manager.pending_execution_metrics() + **tx_execution_metrics,
                );

                if let Some(tx_filter) = &self.deployment_tx_filter {
                    if !(tx.is_l1() || tx.is_protocol_upgrade())
                        && tx_filter
                            .find_not_allowed_deployer(
                                tx.initiator_account(),
                                &tx_result.logs.events,
                            )
                            .await
                            .is_some()
                    {
                        tracing::warn!(
                                "Deployment transaction {tx:?} is not allowed. Mark it as unexecutable."
                            );
                        return Ok((
                            SealResolution::Unexecutable(UnexecutableReason::DeploymentNotAllowed),
                            exec_result,
                        ));
                    }
                }

                let encoding_len = tx.encoding_len();

                let logs_to_apply_iter = tx_result.logs.storage_logs.iter();
                let block_writes_metrics = updates_manager
                    .storage_writes_deduplicator_mut()
                    .apply_and_rollback(logs_to_apply_iter.clone());

                let tx_writes_metrics =
                    StorageWritesDeduplicator::apply_on_empty_state(logs_to_apply_iter);

                let tx_data = SealData {
                    execution_metrics: **tx_execution_metrics,
                    cumulative_size: encoding_len,
                    writes_metrics: tx_writes_metrics,
                    gas_remaining: *gas_remaining,
                };
                let block_data = SealData {
                    execution_metrics: tx_data.execution_metrics
                        + updates_manager.pending_execution_metrics(),
                    cumulative_size: tx_data.cumulative_size
                        + updates_manager.pending_txs_encoding_size(),
                    writes_metrics: block_writes_metrics,
                    gas_remaining: *gas_remaining,
                };
                let is_tx_l1 = tx.is_l1() as usize;

                self.sealer.should_seal_l1_batch(
                    updates_manager.l1_batch_number().0,
                    updates_manager.pending_executed_transactions_len() + 1,
                    updates_manager.pending_l1_transactions_len() + is_tx_l1,
                    &block_data,
                    &tx_data,
                    updates_manager.protocol_version(),
                )
            }
        };
        latency.observe();
        Ok((resolution, exec_result))
    }

    fn report_seal_criteria_capacity(&self, manager: &UpdatesManager) {
        let block_writes_metrics = manager.storage_writes_deduplicator().metrics();

        let block_data = SealData {
            execution_metrics: manager.pending_execution_metrics(),
            cumulative_size: manager.pending_txs_encoding_size(),
            writes_metrics: block_writes_metrics,
            gas_remaining: u32::MAX, // not used
        };

        let capacities = self.sealer.capacity_filled(
            manager.pending_executed_transactions_len(),
            manager.pending_l1_transactions_len(),
            &block_data,
            manager.protocol_version(),
        );
        for (criterion, capacity) in capacities {
            AGGREGATION_METRICS.record_criterion_capacity(criterion, capacity);
        }
    }
}

#[derive(Debug)]
enum ProcessBlockIterationOutcome {
    SealBlock,
    SealBatch,
}

impl StateKeeper {
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        try_stoppable!(self.run_inner(stop_receiver).await);
        Ok(())
    }

    /// Fallible version of `run` routine that allows to easily exit upon cancellation.
    async fn run_inner(
        mut self,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> Result<Infallible, OrStopped> {
        while !is_canceled(&stop_receiver) {
            self.process_block(&mut stop_receiver).await?;
            self.seal_last_pending_block_data().await?;
            self.commit_pending_block().await?;
        }

        Err(OrStopped::Stopped)
    }

    async fn process_block(
        &mut self,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> Result<(), OrStopped> {
        // If there is no open batch then start one.
        let state = self
            .batch_state
            .ensure_initialized(&mut self.inner, stop_receiver)
            .await?;
        let updates_manager = &mut state.updates_manager;
        let batch_executor = state.batch_executor.as_mut();

        if let Some(protocol_upgrade_tx) = state.protocol_upgrade_tx.take() {
            // Protocol upgrade tx if the first tx in the block so we shouldn't do `set_l2_block_params`.
            self.inner
                .process_upgrade_tx(batch_executor, updates_manager, protocol_upgrade_tx)
                .await?;
        } else {
            // Params for the first L2 block in the batch are pushed with batch params.
            // For non-first L2 block they must be set manually.
            if updates_manager.pending_executed_transactions_len() > 0 {
                let next_l2_block_params = self
                    .inner
                    .wait_for_new_l2_block_params(updates_manager, stop_receiver)
                    .await
                    .stop_context("failed getting L2 block params")?;
                StateKeeperInner::set_l2_block_params(updates_manager, next_l2_block_params);
            }

            if state.next_block_should_be_fictive {
                // Create a fictive L2 block.
                let next_l2_block_timestamp =
                    updates_manager.next_l2_block_timestamp_ms_mut().unwrap();
                self.inner
                    .io
                    .update_next_l2_block_timestamp(next_l2_block_timestamp);
                StateKeeperInner::start_next_l2_block(updates_manager, batch_executor).await?;

                return Ok(());
            }
        }

        while !is_canceled(stop_receiver) {
            let full_latency = KEEPER_METRICS.process_block_loop_iteration.start();
            let outcome = Self::process_block_iteration(state, &mut self.inner).await?;
            full_latency.observe();

            match outcome {
                Some(ProcessBlockIterationOutcome::SealBatch) => {
                    state.next_block_should_be_fictive = true;
                    return Ok(());
                }
                Some(ProcessBlockIterationOutcome::SealBlock) => return Ok(()),
                None => {}
            }
        }

        Err(OrStopped::Stopped)
    }

    async fn process_block_iteration(
        state: &mut InitializedBatchState,
        inner: &mut StateKeeperInner,
    ) -> Result<Option<ProcessBlockIterationOutcome>, OrStopped> {
        let updates_manager = &mut state.updates_manager;
        let batch_executor = state.batch_executor.as_mut();

        if inner
            .io
            .should_seal_l1_batch_unconditionally(updates_manager)
            .await?
        {
            // Push the current block if it has not been done yet and this will effectively create a fictive l2 block.
            if let Some(next_l2_block_timestamp) = updates_manager.next_l2_block_timestamp_ms_mut()
            {
                inner
                    .io
                    .update_next_l2_block_timestamp(next_l2_block_timestamp);
                StateKeeperInner::start_next_l2_block(updates_manager, batch_executor).await?;
            }

            tracing::debug!(
                "L2 block #{} should be sealed as per L1 batch unconditional sealing rules",
                updates_manager.last_pending_l2_block().number,
            );
            return Ok(Some(ProcessBlockIterationOutcome::SealBatch));
        }

        if !updates_manager.has_next_block_params()
            && inner.io.should_seal_l2_block(updates_manager)
        {
            tracing::debug!(
                "L2 block #{} should be sealed as per L2 block sealing rules",
                updates_manager.last_pending_l2_block().number,
            );
            return Ok(Some(ProcessBlockIterationOutcome::SealBlock));
        }

        let waiting_latency = KEEPER_METRICS.waiting_for_tx.start();
        if let Some(next_l2_block_timestamp) = updates_manager.next_l2_block_timestamp_ms_mut() {
            // The next block has not started yet, we keep updating the next l2 block parameters with correct timestamp
            inner
                .io
                .update_next_l2_block_timestamp(next_l2_block_timestamp);
        }

        let Some(tx) = inner
            .io
            .wait_for_next_tx(
                POLL_WAIT_DURATION,
                updates_manager.get_next_or_current_l2_block_timestamp(),
            )
            .instrument(info_span!("wait_for_next_tx"))
            .await
            .context("error waiting for next transaction")?
        else {
            waiting_latency.observe();
            tracing::trace!("No new transactions. Waiting!");
            return Ok(None);
        };
        waiting_latency.observe();

        let tx_hash = tx.hash();

        // We need to start a new block
        if updates_manager.has_next_block_params() {
            StateKeeperInner::start_next_l2_block(updates_manager, batch_executor).await?;
        }

        let (seal_resolution, exec_result) = inner
            .process_one_tx(batch_executor, updates_manager, tx.clone())
            .await?;

        let latency = KEEPER_METRICS.match_seal_resolution.start();
        match &seal_resolution {
            SealResolution::NoSeal | SealResolution::IncludeAndSeal => {
                let TxExecutionResult::Success {
                    tx_result,
                    tx_metrics: tx_execution_metrics,
                    call_tracer_result,
                    ..
                } = exec_result
                else {
                    unreachable!(
                        "Tx inclusion seal resolution must be a result of a successful tx execution",
                    );
                };
                updates_manager.extend_from_executed_transaction(
                    tx,
                    *tx_result,
                    *tx_execution_metrics,
                    call_tracer_result,
                );
            }
            SealResolution::ExcludeAndSeal => {
                batch_executor.rollback_last_tx().await.with_context(|| {
                    format!("failed rolling back transaction {tx_hash:?} in batch executor")
                })?;
                inner.io.rollback(tx).await.with_context(|| {
                    format!("failed rolling back transaction {tx_hash:?} in I/O")
                })?;
            }
            SealResolution::Unexecutable(reason) => {
                batch_executor.rollback_last_tx().await.with_context(|| {
                    format!("failed rolling back transaction {tx_hash:?} in batch executor")
                })?;
                inner
                    .io
                    .reject(&tx, reason.clone())
                    .await
                    .with_context(|| format!("cannot reject transaction {tx_hash:?}"))?;
            }
        };
        let result = if seal_resolution.should_seal() {
            tracing::debug!(
                "L2 block #{} should be sealed with conditional sealer resolution {seal_resolution:?} after executing transaction {tx_hash}",
                updates_manager.last_pending_l2_block().number
            );
            Some(ProcessBlockIterationOutcome::SealBatch)
        } else {
            None
        };
        latency.observe();
        Ok(result)
    }

    async fn seal_batch(&mut self) -> anyhow::Result<()> {
        let mut state = self.batch_state.finish();
        assert!(!state.updates_manager.has_next_block_params());

        self.inner
            .report_seal_criteria_capacity(&state.updates_manager);

        let (finished_batch, _) = state.batch_executor.finish_batch().await?;
        state.updates_manager.finish_batch(finished_batch);
        let l1_batch_number = state.updates_manager.l1_batch_number();
        self.inner
            .output_handler
            .handle_l1_batch(Arc::new(state.updates_manager))
            .await
            .with_context(|| format!("failed sealing L1 batch #{l1_batch_number}"))?;

        Ok(())
    }

    async fn seal_last_pending_block_data(&mut self) -> anyhow::Result<()> {
        let state = self.batch_state.unwrap_init_ref();
        let pending_block = state.updates_manager.last_pending_l2_block();
        if !pending_block.executed_transactions.is_empty() {
            self.inner.seal_l2_block(&state.updates_manager).await?;
        }
        Ok(())
    }

    pub async fn commit_pending_block(&mut self) -> anyhow::Result<()> {
        let batch_state = self.batch_state.unwrap_init_mut();
        let pending_block = batch_state.updates_manager.first_pending_l2_block();

        if pending_block.executed_transactions.is_empty() {
            // fictive block -> seal batch.
            self.seal_batch().await?;
        } else {
            if self.inner.leader_rotation {
                // non-fictive block -> finalize block sealing.
                self.inner
                    .output_handler
                    .handle_l2_block_header(
                        &batch_state.updates_manager.header_for_first_pending_block(),
                    )
                    .await?;
                // Important: should come after header is sealed!
                let mut iter = pending_block
                    .executed_transactions
                    .iter()
                    .map(|tx| &tx.transaction);
                self.inner.io.advance_mempool(Box::new(&mut iter)).await;
            }
            batch_state.updates_manager.commit_pending_block();
            batch_state.batch_executor.commit_l2_block().await?;
        }

        Ok(())
    }

    pub async fn rollback(&mut self) -> anyhow::Result<()> {
        let state = self.batch_state.unwrap_init_mut();
        let pending_block = state.updates_manager.last_pending_l2_block();
        tracing::info!("Rolling back block #{}", pending_block.number);

        // Rollback postgres if block is non-fictive.
        // If block is fictive then its data wasn't saved.
        if !pending_block.executed_transactions.is_empty() {
            self.inner
                .output_handler
                .rollback_pending_l2_block_data(pending_block.number)
                .await?;
        }

        // Rollback mempool.
        let txs = pending_block
            .executed_transactions
            .iter()
            .map(|tx| tx.transaction.clone())
            .collect();
        self.inner.io.rollback_l2_block(txs).await?;

        if state.updates_manager.is_in_first_pending_block_state() {
            // Rollback state to `Uninit`.
            state.updates_manager.pop_last_pending_block();
            self.batch_state = BatchState::Uninit(state.updates_manager.io_cursor());
        } else {
            // No need to rollback `state.protocol_upgrade_tx`, since it's only changed in the first block and this is the branch for non-first blocks.
            state.batch_executor.rollback_l2_block().await?;
            // Reset `state.next_block_should_be_fictive` in case non-fictive block is rolled back.
            if !state
                .updates_manager
                .last_pending_l2_block()
                .executed_transactions
                .is_empty()
            {
                state.next_block_should_be_fictive = false;
            }
            state.updates_manager.pop_last_pending_block();
        }

        Ok(())
    }
}

// Test only!!
impl StateKeeper {
    pub async fn run_with_rollback_enabled(
        mut self,
        mut stop_receiver: watch::Receiver<bool>,
        mut block_numbers_to_rollback: VecDeque<L2BlockNumber>,
    ) -> anyhow::Result<()> {
        // Rollback is only enabled for leader rotation mode.
        assert!(self.inner.leader_rotation);

        while !is_canceled(&stop_receiver) {
            try_stoppable!(self.process_block(&mut stop_receiver).await);
            self.seal_last_pending_block_data().await?;

            let block_number = self
                .batch_state
                .unwrap_init_ref()
                .updates_manager
                .last_pending_l2_block()
                .number;
            if block_numbers_to_rollback.front().copied() == Some(block_number) {
                block_numbers_to_rollback.pop_front();
                self.rollback().await?;
            } else {
                self.commit_pending_block().await?;
            }
        }

        Ok(())
    }
}
