use std::{
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};

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
    utils::display_timestamp, InteropRoot, L1BatchNumber, L2BlockNumber, OrStopped, StopContext,
    Transaction,
};
use zksync_vm_executor::whitelist::DeploymentTxFilter;

use crate::{
    executor::TxExecutionResult,
    health::StateKeeperHealthDetails,
    io::{IoCursor, L1BatchParams, L2BlockParams, OutputHandler, PendingBatchData, StateKeeperIO},
    metrics::{AGGREGATION_METRICS, KEEPER_METRICS, L1_BATCH_METRICS},
    seal_criteria::{ConditionalSealer, SealData, SealResolution, UnexecutableReason},
    updates::UpdatesManager,
    utils::is_canceled,
};

/// Amount of time to block on waiting for some resource. The exact value is not really important,
/// we only need it to not block on waiting indefinitely and be able to process cancellation requests.
pub(super) const POLL_WAIT_DURATION: Duration = Duration::from_secs(1);

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
pub struct ZkSyncStateKeeper {
    io: Box<dyn StateKeeperIO>,
    output_handler: OutputHandler,
    batch_executor: Box<dyn BatchExecutorFactory<OwnedStorage>>,
    sealer: Arc<dyn ConditionalSealer>,
    storage_factory: Arc<dyn ReadStorageFactory>,
    health_updater: HealthUpdater,
    deployment_tx_filter: Option<DeploymentTxFilter>,
}

impl ZkSyncStateKeeper {
    pub fn new(
        sequencer: Box<dyn StateKeeperIO>,
        batch_executor: Box<dyn BatchExecutorFactory<OwnedStorage>>,
        output_handler: OutputHandler,
        sealer: Arc<dyn ConditionalSealer>,
        storage_factory: Arc<dyn ReadStorageFactory>,
        deployment_tx_filter: Option<DeploymentTxFilter>,
    ) -> Self {
        Self {
            io: sequencer,
            batch_executor,
            output_handler,
            sealer,
            storage_factory,
            health_updater: ReactiveHealthCheck::new("state_keeper").1,
            deployment_tx_filter,
        }
    }

    pub async fn run(mut self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        try_stoppable!(self.run_inner(stop_receiver).await);
        Ok(())
    }

    /// Fallible version of `run` routine that allows to easily exit upon cancellation.
    async fn run_inner(
        &mut self,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> Result<Infallible, OrStopped> {
        let (cursor, pending_batch_params) = self.io.initialize().await?;
        self.output_handler.initialize(&cursor).await?;
        self.health_updater
            .update(StateKeeperHealthDetails::from(&cursor).into());
        tracing::info!(
            "Starting state keeper. Next l1 batch to seal: {}, next L2 block to seal: {}",
            cursor.l1_batch,
            cursor.next_l2_block
        );

        // Re-execute pending batch if it exists. Otherwise, initialize a new batch.
        let PendingBatchData {
            mut l1_batch_env,
            mut system_env,
            mut pubdata_params,
            pending_l2_blocks,
        } = match pending_batch_params {
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
                params
            }
            None => {
                tracing::info!("There is no open pending batch, starting a new empty batch");
                let (system_env, l1_batch_env, pubdata_params) = self
                    .wait_for_new_batch_env(&cursor, &mut stop_receiver)
                    .await
                    .stop_context("failed getting new batch params")?;
                PendingBatchData {
                    l1_batch_env,
                    pending_l2_blocks: Vec::new(),
                    system_env,
                    pubdata_params,
                }
            }
        };

        let protocol_version = system_env.version;
        let previous_batch_protocol_version = self
            .io
            .load_batch_version_id(l1_batch_env.number - 1)
            .await?;
        let mut updates_manager = UpdatesManager::new(
            &l1_batch_env,
            &system_env,
            pubdata_params,
            previous_batch_protocol_version,
        );
        let mut protocol_upgrade_tx: Option<ProtocolUpgradeTx> = self
            .load_protocol_upgrade_tx(&pending_l2_blocks, protocol_version, l1_batch_env.number)
            .await?;

        let mut batch_executor = self
            .create_batch_executor(
                l1_batch_env.clone(),
                system_env.clone(),
                pubdata_params,
                &stop_receiver,
            )
            .await?;
        self.restore_state(
            &mut *batch_executor,
            &mut updates_manager,
            pending_l2_blocks,
            &stop_receiver,
        )
        .await?;

        let mut l1_batch_seal_delta: Option<Instant> = None;
        while !is_canceled(&stop_receiver) {
            // This function will run until the batch can be sealed.
            self.process_l1_batch(
                &mut *batch_executor,
                &mut updates_manager,
                protocol_upgrade_tx,
                &stop_receiver,
            )
            .await?;
            assert!(!updates_manager.has_next_block_params());

            // Finish current batch with an empty block.
            if !updates_manager.l2_block.executed_transactions.is_empty() {
                self.seal_l2_block(&updates_manager).await?;
                // We've sealed the L2 block that we had, but we still need to set up the timestamp
                // for the fictive L2 block.
                let new_l2_block_params = self
                    .wait_for_new_l2_block_params(&updates_manager, &stop_receiver)
                    .await?;
                Self::set_l2_block_params(&mut updates_manager, new_l2_block_params);
                Self::start_next_l2_block(&mut updates_manager, &mut *batch_executor).await?;
            }

            updates_manager.clear_interop_roots();
            let (finished_batch, _) = batch_executor.finish_batch().await?;
            let sealed_batch_protocol_version = updates_manager.protocol_version();
            updates_manager.finish_batch(finished_batch);
            let mut next_cursor = updates_manager.io_cursor();
            let previous_batch_protocol_version = updates_manager.protocol_version();
            self.output_handler
                .handle_l1_batch(Arc::new(updates_manager))
                .await
                .with_context(|| format!("failed sealing L1 batch {l1_batch_env:?}"))?;

            if let Some(delta) = l1_batch_seal_delta {
                L1_BATCH_METRICS.seal_delta.observe(delta.elapsed());
            }
            l1_batch_seal_delta = Some(Instant::now());

            // Start the new batch.
            next_cursor.l1_batch += 1;
            (system_env, l1_batch_env, pubdata_params) = self
                .wait_for_new_batch_env(&next_cursor, &mut stop_receiver)
                .await?;
            updates_manager = UpdatesManager::new(
                &l1_batch_env,
                &system_env,
                pubdata_params,
                previous_batch_protocol_version,
            );
            batch_executor = self
                .create_batch_executor(
                    l1_batch_env.clone(),
                    system_env.clone(),
                    pubdata_params,
                    &stop_receiver,
                )
                .await?;

            let version_changed = system_env.version != sealed_batch_protocol_version;
            protocol_upgrade_tx = if version_changed {
                self.load_upgrade_tx(system_env.version).await?
            } else {
                None
            };
        }
        Err(OrStopped::Stopped)
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
        Ok(self
            .batch_executor
            .init_batch(storage, l1_batch_env, system_env, pubdata_params))
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

    async fn load_l2_block_interop_root(
        &mut self,
        l2block_number: L2BlockNumber,
    ) -> anyhow::Result<Vec<InteropRoot>> {
        self.io
            .load_l2_block_interop_root(l2block_number)
            .await
            .with_context(|| "failed loading message root".to_string())
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
    async fn wait_for_new_batch_env(
        &mut self,
        cursor: &IoCursor,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> Result<(SystemEnv, L1BatchEnv, PubdataParams), OrStopped> {
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
                Ok(params.into_env(self.io.chain_id(), contracts, cursor, previous_batch_hash))
            }
            _ = stop_receiver.changed() => Err(OrStopped::Stopped),
        }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            l1_batch = %updates.l1_batch.number,
            l2_block = %updates.l2_block.number,
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
            updates_manager.l2_block.number + 1,
            updates_manager.l1_batch.number,
            display_timestamp(l2_block_params.timestamp),
            l2_block_params.virtual_blocks
        );
        updates_manager.set_next_l2_block_params(l2_block_params);
    }

    #[tracing::instrument(
        skip_all,
        fields(
            l1_batch = %updates_manager.l1_batch.number,
            l2_block = %updates_manager.l2_block.number,
        )
    )]
    async fn start_next_l2_block(
        updates_manager: &mut UpdatesManager,
        batch_executor: &mut dyn BatchExecutor<OwnedStorage>,
    ) -> anyhow::Result<()> {
        updates_manager.push_l2_block();
        let block_env = updates_manager.l2_block.get_env();
        tracing::debug!(
            "Initialized new L2 block #{} (L1 batch #{}) with timestamp {}",
            block_env.number,
            updates_manager.l1_batch.number,
            display_timestamp(block_env.timestamp)
        );
        batch_executor
            .start_next_l2_block(block_env.clone())
            .await
            .with_context(|| {
                format!("failed starting L2 block with {block_env:?} in batch executor")
            })
    }

    #[tracing::instrument(
        skip_all,
        fields(
            l1_batch = %updates_manager.l1_batch.number,
            l2_block = %updates_manager.l2_block.number,
        )
    )]
    async fn seal_l2_block(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        self.output_handler
            .handle_l2_block(updates_manager)
            .await
            .with_context(|| {
                format!(
                    "handling L2 block #{} failed",
                    updates_manager.l2_block.number
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
        &mut self,
        batch_executor: &mut dyn BatchExecutor<OwnedStorage>,
        updates_manager: &mut UpdatesManager,
        l2_blocks_to_reexecute: Vec<L2BlockExecutionData>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<(), OrStopped> {
        if l2_blocks_to_reexecute.is_empty() {
            return Ok(());
        }

        for (index, l2_block) in l2_blocks_to_reexecute.into_iter().enumerate() {
            // Push any non-first L2 block to updates manager. The first one was pushed when `updates_manager` was initialized.
            if index > 0 {
                Self::set_l2_block_params(
                    updates_manager,
                    L2BlockParams {
                        timestamp: l2_block.timestamp,
                        virtual_blocks: l2_block.virtual_blocks,
                        interop_roots: self.load_l2_block_interop_root(l2_block.number).await?,
                    },
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
                    l1_batch_number = updates_manager.l1_batch.number,
                    idx_in_l2_block = updates_manager.l2_block.executed_transactions.len(),
                    block_execution_metrics = updates_manager.pending_execution_metrics()
                );
            }
        }

        tracing::debug!(
            "All the transactions from the pending state were re-executed successfully"
        );

        // We've processed all the L2 blocks, and right now we're preparing the next *actual* L2 block.
        // The `wait_for_new_l2_block_params` call is used to initialize the StateKeeperIO with a correct new l2 block params
        let new_l2_block_params = self
            .wait_for_new_l2_block_params(updates_manager, stop_receiver)
            .await
            .stop_context("failed getting L2 block params")?;
        Self::set_l2_block_params(updates_manager, new_l2_block_params);
        Ok(())
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %updates_manager.l1_batch.number)
    )]
    async fn process_l1_batch(
        &mut self,
        batch_executor: &mut dyn BatchExecutor<OwnedStorage>,
        updates_manager: &mut UpdatesManager,
        protocol_upgrade_tx: Option<ProtocolUpgradeTx>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<(), OrStopped> {
        if let Some(protocol_upgrade_tx) = protocol_upgrade_tx {
            self.process_upgrade_tx(batch_executor, updates_manager, protocol_upgrade_tx)
                .await?;
        }

        while !is_canceled(stop_receiver) {
            let full_latency = KEEPER_METRICS.process_l1_batch_loop_iteration.start();

            if self
                .io
                .should_seal_l1_batch_unconditionally(updates_manager)
                .await?
            {
                tracing::debug!(
                    "L1 batch #{} should be sealed unconditionally as per sealing rules",
                    updates_manager.l1_batch.number
                );

                // Push the current block if it has not been done yet and this will effectively create a fictive l2 block
                if let Some(next_l2_block_timestamp) = updates_manager.next_l2_block_timestamp_mut()
                {
                    self.io
                        .update_next_l2_block_timestamp(next_l2_block_timestamp);
                    Self::start_next_l2_block(updates_manager, batch_executor).await?;
                }

                self.report_seal_criteria_capacity(updates_manager);
                full_latency.observe();

                return Ok(());
            }

            if !updates_manager.has_next_block_params()
                && self.io.should_seal_l2_block(updates_manager)
            {
                tracing::debug!(
                    "L2 block #{} (L1 batch #{}) should be sealed as per sealing rules",
                    updates_manager.l2_block.number,
                    updates_manager.l1_batch.number
                );

                self.seal_l2_block(updates_manager).await?;

                // Get a tentative new l2 block parameters
                let mut next_l2_block_params = self
                    .wait_for_new_l2_block_params(updates_manager, stop_receiver)
                    .await
                    .stop_context("failed getting L2 block params")?;
                next_l2_block_params.interop_roots = self.io.load_latest_interop_root().await?;
                Self::set_l2_block_params(updates_manager, next_l2_block_params);
            }

            let waiting_latency = KEEPER_METRICS.waiting_for_tx.start();

            if let Some(next_l2_block_timestamp) = updates_manager.next_l2_block_timestamp_mut() {
                // The next block has not started yet, we keep updating the next l2 block parameters with correct timestamp
                self.io
                    .update_next_l2_block_timestamp(next_l2_block_timestamp);
            }

            let Some(tx) = self
                .io
                .wait_for_next_tx(
                    POLL_WAIT_DURATION,
                    updates_manager
                        .get_next_l2_block_params_or_batch_params()
                        .timestamp,
                )
                .instrument(info_span!("wait_for_next_tx"))
                .await
                .context("error waiting for next transaction")?
            else {
                waiting_latency.observe();
                tracing::trace!("No new transactions. Waiting!");
                continue;
            };
            waiting_latency.observe();

            let tx_hash = tx.hash();

            // We need to start a new block
            if updates_manager.has_next_block_params() {
                Self::start_next_l2_block(updates_manager, batch_executor).await?;
            }

            let (seal_resolution, exec_result) = self
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
                    self.io.rollback(tx).await.with_context(|| {
                        format!("failed rolling back transaction {tx_hash:?} in I/O")
                    })?;
                }
                SealResolution::Unexecutable(reason) => {
                    batch_executor.rollback_last_tx().await.with_context(|| {
                        format!("failed rolling back transaction {tx_hash:?} in batch executor")
                    })?;
                    self.io
                        .reject(&tx, reason.clone())
                        .await
                        .with_context(|| format!("cannot reject transaction {tx_hash:?}"))?;
                }
            };
            latency.observe();

            if seal_resolution.should_seal() {
                tracing::debug!(
                    "L1 batch #{} should be sealed with resolution {seal_resolution:?} after executing \
                     transaction {tx_hash}",
                    updates_manager.l1_batch.number
                );
                self.report_seal_criteria_capacity(updates_manager);
                full_latency.observe();
                return Ok(());
            }
            full_latency.observe();
        }
        Err(OrStopped::Stopped)
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
                    updates_manager.l1_batch.number,
                    updates_manager.l2_block.executed_transactions.len() + 1,
                    updates_manager.l2_block.number,
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
                    .storage_writes_deduplicator
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
                    updates_manager.l1_batch.number.0,
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

    /// Returns the health check for state keeper.
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    fn report_seal_criteria_capacity(&self, manager: &UpdatesManager) {
        let block_writes_metrics = manager.storage_writes_deduplicator.metrics();

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
