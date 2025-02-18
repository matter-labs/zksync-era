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
    utils::display_timestamp, Transaction,
};

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

/// Structure used to indicate that task cancellation was requested.
#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    #[error("canceled")]
    Canceled,
    #[error(transparent)]
    Fatal(#[from] anyhow::Error),
}

impl Error {
    fn context(self, msg: &'static str) -> Self {
        match self {
            Self::Canceled => Self::Canceled,
            Self::Fatal(err) => Self::Fatal(err.context(msg)),
        }
    }
}

/// State keeper represents a logic layer of L1 batch / L2 block processing flow.
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
}

impl ZkSyncStateKeeper {
    pub fn new(
        sequencer: Box<dyn StateKeeperIO>,
        batch_executor: Box<dyn BatchExecutorFactory<OwnedStorage>>,
        output_handler: OutputHandler,
        sealer: Arc<dyn ConditionalSealer>,
        storage_factory: Arc<dyn ReadStorageFactory>,
    ) -> Self {
        Self {
            io: sequencer,
            batch_executor,
            output_handler,
            sealer,
            storage_factory,
            health_updater: ReactiveHealthCheck::new("state_keeper").1,
        }
    }

    pub async fn run(mut self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        match self.run_inner(stop_receiver).await {
            Ok(_) => unreachable!(),
            Err(Error::Fatal(err)) => Err(err).context("state_keeper failed"),
            Err(Error::Canceled) => {
                tracing::info!("Stop signal received, state keeper is shutting down");
                Ok(())
            }
        }
    }

    /// Fallible version of `run` routine that allows to easily exit upon cancellation.
    async fn run_inner(
        &mut self,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> Result<Infallible, Error> {
        let (cursor, pending_batch_params) = self.io.initialize().await?;
        self.output_handler.initialize(&cursor).await?;
        self.health_updater
            .update(StateKeeperHealthDetails::from(&cursor).into());
        tracing::info!(
            "Starting state keeper. Next l1 batch to seal: {}, next L2 block to seal: {}",
            cursor.l1_batch,
            cursor.next_l2_block
        );

        let mut batch_first_tx: Option<Transaction>;
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
                // In case of pending state, the first transaction has already been executed
                batch_first_tx = None;
                params
            }
            None => {
                tracing::info!("There is no open pending batch, starting a new empty batch");
                let (system_env, l1_batch_env, pubdata_params, tx) = self
                    .wait_for_new_batch_env(&cursor, &mut stop_receiver)
                    .await
                    .map_err(|e| e.context("wait_for_new_batch_params()"))?;
                batch_first_tx = tx;
                PendingBatchData {
                    l1_batch_env,
                    pending_l2_blocks: Vec::new(),
                    system_env,
                    pubdata_params,
                }
            }
        };

        let mut updates_manager = UpdatesManager::new(&l1_batch_env, &system_env, pubdata_params);
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
                batch_first_tx,
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

            let (finished_batch, _) = batch_executor.finish_batch().await?;
            updates_manager.finish_batch(finished_batch);
            let mut next_cursor = updates_manager.io_cursor();
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
            (system_env, l1_batch_env, pubdata_params, batch_first_tx) = self
                .wait_for_new_batch_env(&next_cursor, &mut stop_receiver)
                .await?;
            updates_manager = UpdatesManager::new(&l1_batch_env, &system_env, pubdata_params);
            batch_executor = self
                .create_batch_executor(
                    l1_batch_env.clone(),
                    system_env.clone(),
                    pubdata_params,
                    &stop_receiver,
                )
                .await?;
        }
        Err(Error::Canceled)
    }

    async fn create_batch_executor(
        &mut self,
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        pubdata_params: PubdataParams,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<Box<dyn BatchExecutor<OwnedStorage>>, Error> {
        let storage = self
            .storage_factory
            .access_storage(stop_receiver, l1_batch_env.number - 1)
            .await
            .context("failed creating VM storage")?
            .ok_or(Error::Canceled)?;
        Ok(self
            .batch_executor
            .init_batch(storage, l1_batch_env, system_env, pubdata_params))
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
    ) -> Result<L1BatchParams, Error> {
        while !is_canceled(stop_receiver) {
            if let Some(params) = self
                .io
                .wait_for_new_batch_params(cursor, POLL_WAIT_DURATION)
                .await?
            {
                return Ok(params);
            }
        }
        Err(Error::Canceled)
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
    ) -> Result<(SystemEnv, L1BatchEnv, PubdataParams, Option<Transaction>), Error> {
        // `io.wait_for_new_batch_params(..)` is not cancel-safe; once we get new batch params, we must hold onto them
        // until we get the rest of parameters from I/O or receive a stop signal.
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
            _ = stop_receiver.changed() => Err(Error::Canceled),
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
    ) -> Result<L2BlockParams, Error> {
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
        Err(Error::Canceled)
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
            .start_next_l2_block(block_env)
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
    ) -> Result<(), Error> {
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
            .map_err(|e| e.context("wait_for_new_l2_block_params"))?;
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
        batch_first_tx: Option<Transaction>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<(), Error> {
        if let Some(tx) = batch_first_tx {
            self.process_first_tx(batch_executor, updates_manager, tx)
                .await?;
        }

        while !is_canceled(stop_receiver) {
            let full_latency = KEEPER_METRICS.process_l1_batch_loop_iteration.start();

            if self
                .io
                .should_seal_l1_batch_unconditionally(updates_manager)
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
                let next_l2_block_params = self
                    .wait_for_new_l2_block_params(updates_manager, stop_receiver)
                    .await
                    .map_err(|e| e.context("wait_for_new_l2_block_params"))?;
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
                full_latency.observe();
                return Ok(());
            }
            full_latency.observe();
        }
        Err(Error::Canceled)
    }

    async fn process_first_tx(
        &mut self,
        batch_executor: &mut dyn BatchExecutor<OwnedStorage>,
        updates_manager: &mut UpdatesManager,
        first_tx: Transaction,
    ) -> anyhow::Result<()> {
        // Sanity check: tx must be the first one in the batch.
        assert_eq!(updates_manager.pending_executed_transactions_len(), 0);
        let is_upgrade_tx = first_tx.tx_format() == TransactionType::ProtocolUpgradeTransaction;
        let first_tx_hash = first_tx.hash();

        let (seal_resolution, exec_result) = self
            .process_one_tx(batch_executor, updates_manager, first_tx.clone())
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
                if is_upgrade_tx && tx_result.result.is_failed() {
                    anyhow::bail!("Failed upgrade tx {:?}", first_tx_hash);
                }
                updates_manager.extend_from_executed_transaction(
                    first_tx,
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
                if is_upgrade_tx {
                    anyhow::bail!(
                        "Upgrade transaction {:?} is unexecutable: {reason}",
                        first_tx.hash()
                    );
                } else {
                    batch_executor.rollback_last_tx().await.with_context(|| {
                        format!(
                            "failed rolling back transaction {first_tx_hash:?} in batch executor"
                        )
                    })?;
                    self.io.reject(&first_tx, reason.clone()).await
                }
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
                    updates_manager.batch_timestamp() as u128 * 1_000,
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
}
