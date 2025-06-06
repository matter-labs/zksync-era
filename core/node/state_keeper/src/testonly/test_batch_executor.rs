// TODO(QIT-33): Some of the interfaces are public, and some are only used in tests within this crate.
// This causes crate-local interfaces to spawn a warning without `cfg(test)`. The interfaces here must
// be revisited and properly split into "truly public" (e.g. useful for other crates to test, say, different
// IO or `BatchExecutor` implementations) and "local-test-only" (e.g. used only in tests within this crate).
#![allow(dead_code)]

use std::{
    collections::{HashMap, HashSet, VecDeque},
    convert::TryInto,
    fmt, mem,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use tokio::sync::watch;
use zksync_contracts::BaseSystemContracts;
use zksync_multivm::{
    interface::{
        executor::{BatchExecutor, BatchExecutorFactory},
        storage::InMemoryStorage,
        BatchTransactionExecutionResult, ExecutionResult, FinishedL1Batch, Halt, L1BatchEnv,
        L2BlockEnv, SystemEnv, VmExecutionLogs, VmExecutionResultAndLogs,
    },
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};
use zksync_node_test_utils::create_l2_transaction;
use zksync_state::{interface::StorageView, OwnedStorage, ReadStorageFactory};
use zksync_types::{
    commitment::PubdataParams, fee_model::BatchFeeInput, l2_to_l1_log::UserL2ToL1Log,
    protocol_upgrade::ProtocolUpgradeTx, Address, L1BatchNumber, L2BlockNumber, L2ChainId,
    OrStopped, ProtocolVersionId, Transaction, H256,
};

use crate::{
    io::{IoCursor, L1BatchParams, L2BlockParams, PendingBatchData, StateKeeperIO},
    seal_criteria::{IoSealCriteria, SequencerSealer, UnexecutableReason},
    testonly::{successful_exec, BASE_SYSTEM_CONTRACTS},
    updates::UpdatesManager,
    OutputHandler, StateKeeperInner, StateKeeperOutputHandler,
};

pub const FEE_ACCOUNT: Address = Address::repeat_byte(0x11);

/// Main entry for writing tests for the state keeper.
/// Represents a planned sequence of actions that would happen with the state keeper.
/// We defined a scenario by telling *exactly* what we expect to happen, and then launch the state keeper.
/// While state keeper progresses over the planned transactions, `TestScenario` makes sure that every action happens
/// according to the scenario.
///
/// Every action requires a description: since in most scenarios there will be a lot of similar actions (e.g. `next_tx`
/// or `seal_l2_block`) it helps to see which action *exactly* caused a test failure. It's recommended to write
/// descriptions that are not only unique, but also will explain *why* we expected this action to happen. This way,
/// it would be easier for developer to find the problem.
///
/// See any test in the `mod.rs` file to get a visual example.
pub(crate) struct TestScenario {
    actions: VecDeque<ScenarioItem>,
    pending_batch: Option<PendingBatchData>,
    l1_batch_seal_fn: Box<SealFn>,
    l2_block_seal_fn: Box<SealFn>,
}

type SealFn = dyn FnMut(&UpdatesManager) -> bool + Send + Sync;

impl fmt::Debug for TestScenario {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TestScenario")
            .field("actions", &self.actions)
            .field("pending_batch", &self.pending_batch)
            .finish_non_exhaustive()
    }
}

impl TestScenario {
    pub(crate) fn new() -> Self {
        Self {
            actions: VecDeque::new(),
            pending_batch: None,
            l1_batch_seal_fn: Box::new(|_| false),
            l2_block_seal_fn: Box::new(|_| false),
        }
    }

    /// Adds a pending batch data that would be fed into the state keeper.
    /// Note that during processing pending batch, state keeper do *not* call `seal_l2_block` method on the IO (since
    /// it only recovers the temporary state).
    pub(crate) fn load_pending_batch(mut self, pending_batch: PendingBatchData) -> Self {
        self.pending_batch = Some(pending_batch);
        self
    }

    /// Configures scenario to repeatedly return `None` to tx requests until the next action from the scenario happens.
    pub(crate) fn no_txs_until_next_action(mut self, description: &'static str) -> Self {
        self.actions
            .push_back(ScenarioItem::NoTxsUntilNextAction(description));
        self
    }

    /// Increments protocol version returned by IO.
    pub(crate) fn increment_protocol_version(mut self, description: &'static str) -> Self {
        self.actions
            .push_back(ScenarioItem::IncrementProtocolVersion(description));
        self
    }

    /// Expect the state keeper to request a transaction from IO.
    /// Adds both a transaction and an outcome of this transaction (that would be returned to the state keeper from the
    /// batch executor).
    pub(crate) fn next_tx(
        mut self,
        description: &'static str,
        tx: Transaction,
        result: BatchTransactionExecutionResult,
    ) -> Self {
        self.actions
            .push_back(ScenarioItem::Tx(description, tx, result));
        self
    }

    /// Expect the state keeper to rollback the transaction (i.e. return to the mempool).
    pub(crate) fn tx_rollback(mut self, description: &'static str, tx: Transaction) -> Self {
        self.actions
            .push_back(ScenarioItem::Rollback(description, tx));
        self
    }

    /// Expect the state keeper to reject the transaction.
    /// `err` argument is an optional substring of the expected error message. If `None` is provided, any rejection
    /// would work. If `Some` is provided, rejection reason would be checked against the provided substring.
    pub(crate) fn tx_rejected(
        mut self,
        description: &'static str,
        tx: Transaction,
        err: UnexecutableReason,
    ) -> Self {
        self.actions
            .push_back(ScenarioItem::Reject(description, tx, err));
        self
    }

    /// Expects the L2 block to be sealed.
    pub(crate) fn l2_block_sealed(mut self, description: &'static str) -> Self {
        self.actions
            .push_back(ScenarioItem::L2BlockSeal(description, None));
        self
    }

    /// Expects the L2 block to be sealed.
    /// Accepts a function that would be given access to the received L2 block seal params, which can implement
    /// additional assertions on the sealed L2 block.
    pub(crate) fn l2_block_sealed_with<F: FnOnce(&UpdatesManager) + Send + 'static>(
        mut self,
        description: &'static str,
        f: F,
    ) -> Self {
        self.actions
            .push_back(ScenarioItem::L2BlockSeal(description, Some(Box::new(f))));
        self
    }

    /// Expects the batch to be sealed.
    pub(crate) fn batch_sealed(mut self, description: &'static str) -> Self {
        self.actions
            .push_back(ScenarioItem::BatchSeal(description, None));
        self
    }

    /// Expects the batch to be sealed.
    /// Accepts a function that would be given access to the received batch seal params, which can implement
    /// additional assertions on the sealed batch.
    pub(crate) fn batch_sealed_with<F>(mut self, description: &'static str, f: F) -> Self
    where
        F: FnOnce(&UpdatesManager) + Send + 'static,
    {
        self.actions
            .push_back(ScenarioItem::BatchSeal(description, Some(Box::new(f))));
        self
    }

    pub(crate) fn seal_l1_batch_when<F>(mut self, seal_fn: F) -> Self
    where
        F: FnMut(&UpdatesManager) -> bool + Send + Sync + 'static,
    {
        self.l1_batch_seal_fn = Box::new(seal_fn);
        self
    }

    pub(crate) fn seal_l2_block_when<F>(mut self, seal_fn: F) -> Self
    where
        F: FnMut(&UpdatesManager) -> bool + Send + Sync + 'static,
    {
        self.l2_block_seal_fn = Box::new(seal_fn);
        self
    }

    pub(crate) fn update_l2_block_timestamp(
        mut self,
        description: &'static str,
        new_timestamp: u64,
    ) -> Self {
        self.actions.push_back(ScenarioItem::UpdateBlockTimestamp(
            description,
            new_timestamp,
        ));
        self
    }

    /// Launches the test.
    /// Provided `SealManager` is expected to be externally configured to adhere the written scenario logic.
    pub(crate) async fn run(self, sealer: SequencerSealer) {
        assert!(!self.actions.is_empty(), "Test scenario can't be empty");

        let batch_executor = TestBatchExecutorBuilder::new(&self);
        let (stop_sender, stop_receiver) = watch::channel(false);
        let (io, output_handler) = TestIO::new(stop_sender, self);
        let state_keeper_inner = StateKeeperInner::new(
            Box::new(io),
            Box::new(batch_executor),
            output_handler,
            Arc::new(sealer),
            Arc::new(MockReadStorageFactory),
            None,
        );
        let state_keeper = state_keeper_inner.initialize(&stop_receiver).await.unwrap();
        let sk_thread = tokio::spawn(state_keeper.run(stop_receiver));

        // We must assume that *theoretically* state keeper may ignore the stop request from IO once scenario is
        // completed, so we spawn it in a separate thread to not get test stuck.
        let hard_timeout = Duration::from_secs(60);
        let poll_interval = Duration::from_millis(50);
        let start = Instant::now();
        while start.elapsed() <= hard_timeout {
            if sk_thread.is_finished() {
                sk_thread
                    .await
                    .unwrap_or_else(|_| panic!("State keeper thread panicked"))
                    .unwrap();
                return;
            }
            tokio::time::sleep(poll_interval).await;
        }
        panic!("State keeper test did not exit until the hard timeout, probably it got stuck");
    }
}

/// Creates a random transaction. Provided tx number would be used as a transaction hash,
/// so it's easier to understand which transaction caused test to fail.
pub(crate) fn random_tx(tx_number: u64) -> Transaction {
    let mut tx = create_l2_transaction(10, 100);
    // Set the `tx_number` as tx hash so if transaction causes problems,
    // it'll be easier to understand which one.
    tx.set_input(H256::random().0.to_vec(), H256::from_low_u64_be(tx_number));
    tx.into()
}

/// Creates a random protocol upgrade transaction. Provided tx number would be used as a transaction hash,
/// so it's easier to understand which transaction caused test to fail.
pub(crate) fn random_upgrade_tx(tx_number: u64) -> ProtocolUpgradeTx {
    let mut tx = ProtocolUpgradeTx {
        execute: Default::default(),
        common_data: Default::default(),
        received_timestamp_ms: 0,
    };
    tx.common_data.canonical_tx_hash = H256::from_low_u64_be(tx_number);
    tx
}

/// Creates a `TxExecutionResult` object denoting a successful tx execution with the given execution metrics.
pub(crate) fn successful_exec_with_log() -> BatchTransactionExecutionResult {
    BatchTransactionExecutionResult {
        tx_result: Box::new(VmExecutionResultAndLogs {
            logs: VmExecutionLogs {
                user_l2_to_l1_logs: vec![UserL2ToL1Log::default()],
                ..VmExecutionLogs::default()
            },
            ..VmExecutionResultAndLogs::mock_success()
        }),
        compression_result: Ok(()),
        call_traces: vec![],
    }
}

/// Creates a `TxExecutionResult` object denoting a tx that was rejected.
pub(crate) fn rejected_exec(reason: Halt) -> BatchTransactionExecutionResult {
    BatchTransactionExecutionResult {
        tx_result: Box::new(VmExecutionResultAndLogs::mock(ExecutionResult::Halt {
            reason,
        })),
        compression_result: Ok(()),
        call_traces: vec![],
    }
}

#[allow(clippy::type_complexity, clippy::large_enum_variant)] // It's OK for tests.
enum ScenarioItem {
    /// Configures scenario to repeatedly return `None` to tx requests until the next action from the scenario happens.
    NoTxsUntilNextAction(&'static str),
    /// Increments protocol version in IO state.
    IncrementProtocolVersion(&'static str),
    Tx(&'static str, Transaction, BatchTransactionExecutionResult),
    Rollback(&'static str, Transaction),
    Reject(&'static str, Transaction, UnexecutableReason),
    L2BlockSeal(
        &'static str,
        Option<Box<dyn FnOnce(&UpdatesManager) + Send>>,
    ),
    BatchSeal(
        &'static str,
        Option<Box<dyn FnOnce(&UpdatesManager) + Send>>,
    ),
    /// Update block timestamp with a new timestamp.
    UpdateBlockTimestamp(&'static str, u64),
}

impl fmt::Debug for ScenarioItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTxsUntilNextAction(descr) => formatter
                .debug_tuple("NoTxsUntilNextAction")
                .field(descr)
                .finish(),
            Self::IncrementProtocolVersion(descr) => formatter
                .debug_tuple("IncrementProtocolVersion")
                .field(descr)
                .finish(),
            Self::Tx(descr, tx, result) => formatter
                .debug_tuple("Tx")
                .field(descr)
                .field(tx)
                .field(result)
                .finish(),
            Self::Rollback(descr, tx) => formatter
                .debug_tuple("Rollback")
                .field(descr)
                .field(tx)
                .finish(),
            Self::Reject(descr, tx, err) => formatter
                .debug_tuple("Reject")
                .field(descr)
                .field(tx)
                .field(err)
                .finish(),
            Self::L2BlockSeal(descr, _) => {
                formatter.debug_tuple("L2BlockSeal").field(descr).finish()
            }
            Self::BatchSeal(descr, _) => formatter.debug_tuple("BatchSeal").field(descr).finish(),
            Self::UpdateBlockTimestamp(descr, timestamp) => formatter
                .debug_tuple("UpdateBlockTimestamp")
                .field(descr)
                .field(timestamp)
                .finish(),
        }
    }
}

type ExpectedTransactions = VecDeque<HashMap<H256, VecDeque<BatchTransactionExecutionResult>>>;

#[derive(Debug, Default)]
pub struct TestBatchExecutorBuilder {
    /// Sequence of known transaction execution results per batch.
    /// We need to store txs for each batch separately, since the same transaction
    /// can be executed in several batches (e.g. after an `ExcludeAndSeal` rollback).
    /// When initializing each batch, we will `pop_front` known txs for the corresponding executor.
    txs: ExpectedTransactions,
    /// Set of transactions that would be rolled back at least once.
    rollback_set: HashSet<H256>,
}

impl TestBatchExecutorBuilder {
    pub(crate) fn new(scenario: &TestScenario) -> Self {
        let mut txs = VecDeque::new();
        let mut batch_txs = HashMap::<_, VecDeque<BatchTransactionExecutionResult>>::new();
        let mut rollback_set = HashSet::new();

        // Insert data about the pending batch, if it exists.
        // All the txs from the pending batch must succeed.
        if let Some(pending_batch) = &scenario.pending_batch {
            for tx in pending_batch
                .pending_l2_blocks
                .iter()
                .flat_map(|l2_block| &l2_block.txs)
            {
                batch_txs.insert(tx.hash(), vec![successful_exec()].into());
            }
        }

        // Go through scenario and collect per-batch transactions and the overall rollback set.
        for item in &scenario.actions {
            match item {
                ScenarioItem::Tx(_, tx, result) => {
                    result.compression_result.as_ref().unwrap();
                    let result = BatchTransactionExecutionResult {
                        tx_result: result.tx_result.clone(),
                        compression_result: Ok(()),
                        call_traces: result.call_traces.clone(),
                    };

                    if let Some(txs) = batch_txs.get_mut(&tx.hash()) {
                        txs.push_back(result);
                    } else {
                        batch_txs.insert(tx.hash(), VecDeque::from([result]));
                    }
                }
                ScenarioItem::Rollback(_, tx) => {
                    rollback_set.insert(tx.hash());
                }
                ScenarioItem::Reject(_, tx, _) => {
                    rollback_set.insert(tx.hash());
                }
                ScenarioItem::BatchSeal(_, _) => txs.push_back(mem::take(&mut batch_txs)),
                _ => {}
            }
        }

        // Some batch seal may not be included into scenario, dump such txs if they exist.
        if !batch_txs.is_empty() {
            txs.push_back(mem::take(&mut batch_txs));
        }
        // After sealing the batch, state keeper initialized a new one, so we need to create an empty set
        // for the initialization of the "next-to-last" batch.
        txs.push_back(HashMap::default());

        Self { txs, rollback_set }
    }

    /// Adds successful transactions to be executed in a single L1 batch.
    pub fn push_successful_transactions(&mut self, tx_hashes: &[H256]) {
        let txs = tx_hashes
            .iter()
            .copied()
            .map(|tx_hash| (tx_hash, VecDeque::from([successful_exec()])));
        self.txs.push_back(txs.collect());
    }
}

impl BatchExecutorFactory<OwnedStorage> for TestBatchExecutorBuilder {
    fn init_batch(
        &mut self,
        _storage: OwnedStorage,
        _l1_batch_env: L1BatchEnv,
        _system_env: SystemEnv,
        _pubdata_params: PubdataParams,
    ) -> Box<dyn BatchExecutor<OwnedStorage>> {
        let executor =
            TestBatchExecutor::new(self.txs.pop_front().unwrap(), self.rollback_set.clone());
        Box::new(executor)
    }
}

#[derive(Debug)]
pub(super) struct TestBatchExecutor {
    /// Mapping tx -> response.
    /// The same transaction can be executed several times, so we use a sequence of responses and consume them by one.
    txs: HashMap<H256, VecDeque<BatchTransactionExecutionResult>>,
    /// Set of transactions that are expected to be rolled back.
    rollback_set: HashSet<H256>,
    /// Last executed tx hash.
    last_tx: H256,
}

impl TestBatchExecutor {
    pub(super) fn new(
        txs: HashMap<H256, VecDeque<BatchTransactionExecutionResult>>,
        rollback_set: HashSet<H256>,
    ) -> Self {
        Self {
            txs,
            rollback_set,
            last_tx: H256::default(), // We don't expect rollbacks until the first tx is executed.
        }
    }
}

#[async_trait]
impl BatchExecutor<OwnedStorage> for TestBatchExecutor {
    async fn execute_tx(
        &mut self,
        tx: Transaction,
    ) -> anyhow::Result<BatchTransactionExecutionResult> {
        let result = self
            .txs
            .get_mut(&tx.hash())
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                panic!(
                    "Received a request to execute an unknown transaction: {:?}",
                    tx
                )
            });
        self.last_tx = tx.hash();
        Ok(result)
    }

    async fn rollback_last_tx(&mut self) -> anyhow::Result<()> {
        // This is an additional safety check: IO would check that every rollback is included in the
        // test scenario, but here we want to additionally check that each such request goes to the
        // the batch executor as well.
        if !self.rollback_set.contains(&self.last_tx) {
            // Request to rollback an unexpected tx.
            panic!(
                "Received a request to rollback an unexpected tx. Last executed tx: {:?}",
                self.last_tx
            )
        }
        // It's OK to not update `last_executed_tx`, since state keeper never should rollback more than 1
        // tx in a row, and it's going to cause a panic anyway.
        Ok(())
    }

    async fn start_next_l2_block(&mut self, _env: L2BlockEnv) -> anyhow::Result<()> {
        Ok(())
    }

    async fn finish_batch(
        self: Box<Self>,
    ) -> anyhow::Result<(FinishedL1Batch, StorageView<OwnedStorage>)> {
        let storage = OwnedStorage::boxed(InMemoryStorage::default());
        Ok((FinishedL1Batch::mock(), StorageView::new(storage)))
    }

    async fn gas_remaining(&mut self) -> anyhow::Result<u32> {
        Ok(u32::MAX)
    }
}

#[derive(Debug)]
pub(super) struct TestPersistence {
    actions: Arc<Mutex<VecDeque<ScenarioItem>>>,
    stop_sender: Arc<watch::Sender<bool>>,
}

impl TestPersistence {
    fn pop_next_item(&self, request: &str) -> ScenarioItem {
        let mut actions = self.actions.lock().expect("scenario queue is poisoned");
        let action = actions
            .pop_front()
            .unwrap_or_else(|| panic!("no action for request: {request}"));
        // If that was a last action, tell the state keeper to stop after that.
        if actions.is_empty() {
            self.stop_sender.send_replace(true);
        }
        action
    }
}

#[async_trait]
impl StateKeeperOutputHandler for TestPersistence {
    async fn handle_l2_block(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        let action = self.pop_next_item("seal_l2_block");
        let ScenarioItem::L2BlockSeal(_, check_fn) = action else {
            anyhow::bail!("Unexpected action: {:?}", action);
        };
        if let Some(check_fn) = check_fn {
            check_fn(updates_manager);
        }
        Ok(())
    }

    async fn handle_l1_batch(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        let action = self.pop_next_item("seal_l1_batch");
        let ScenarioItem::BatchSeal(_, check_fn) = action else {
            anyhow::bail!("Unexpected action: {:?}", action);
        };
        if let Some(check_fn) = check_fn {
            check_fn(&updates_manager);
        }
        Ok(())
    }
}

pub(crate) struct TestIO {
    stop_sender: Arc<watch::Sender<bool>>,
    batch_number: L1BatchNumber,
    timestamp: u64,
    fee_input: BatchFeeInput,
    l2_block_number: L2BlockNumber,
    fee_account: Address,
    pending_batch: Option<PendingBatchData>,
    l1_batch_seal_fn: Box<SealFn>,
    l2_block_seal_fn: Box<SealFn>,
    actions: Arc<Mutex<VecDeque<ScenarioItem>>>,
    /// Internal flag that is being set if scenario was configured to return `None` to all the transaction
    /// requests until some other action happens.
    skipping_txs: bool,
    protocol_version: ProtocolVersionId,
    previous_batch_protocol_version: ProtocolVersionId,
    protocol_upgrade_txs: HashMap<ProtocolVersionId, ProtocolUpgradeTx>,
}

impl fmt::Debug for TestIO {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("TestIO").finish_non_exhaustive()
    }
}

impl TestIO {
    pub(crate) fn new(
        stop_sender: watch::Sender<bool>,
        scenario: TestScenario,
    ) -> (Self, OutputHandler) {
        let stop_sender = Arc::new(stop_sender);
        let actions = Arc::new(Mutex::new(scenario.actions));
        let persistence = TestPersistence {
            stop_sender: stop_sender.clone(),
            actions: actions.clone(),
        };

        let (l2_block_number, timestamp) = if let Some(pending_batch) = &scenario.pending_batch {
            let last_pending_l2_block = pending_batch
                .pending_l2_blocks
                .last()
                .expect("pending batch should have at least one L2 block");
            (
                last_pending_l2_block.number + 1,
                last_pending_l2_block.timestamp + 1,
            )
        } else {
            (L2BlockNumber(1), 1)
        };
        let this = Self {
            stop_sender,
            batch_number: L1BatchNumber(1),
            timestamp,
            fee_input: BatchFeeInput::default(),
            pending_batch: scenario.pending_batch,
            l1_batch_seal_fn: scenario.l1_batch_seal_fn,
            l2_block_seal_fn: scenario.l2_block_seal_fn,
            actions,
            l2_block_number,
            fee_account: FEE_ACCOUNT,
            skipping_txs: false,
            protocol_version: ProtocolVersionId::latest(),
            previous_batch_protocol_version: ProtocolVersionId::latest(),
            protocol_upgrade_txs: HashMap::default(),
        };
        (this, OutputHandler::new(Box::new(persistence)))
    }

    pub fn add_upgrade_tx(&mut self, version: ProtocolVersionId, tx: ProtocolUpgradeTx) {
        self.protocol_upgrade_txs.insert(version, tx);
    }

    fn pop_next_item(&mut self, request: &str) -> ScenarioItem {
        let mut actions = self.actions.lock().expect("scenario queue is poisoned");
        loop {
            let action = actions.pop_front().unwrap_or_else(|| {
                panic!(
                    "Test scenario is empty, but the following action was done by the state keeper: {request}"
                );
            });
            // If that was a last action, tell the state keeper to stop after that.
            if actions.is_empty() {
                self.stop_sender.send_replace(true);
            }

            match &action {
                ScenarioItem::NoTxsUntilNextAction(_) => {
                    self.skipping_txs = true;
                    // This is a mock item, so pop an actual one for the IO to process.
                    continue;
                }
                ScenarioItem::IncrementProtocolVersion(_) => {
                    self.protocol_version = (self.protocol_version as u16 + 1)
                        .try_into()
                        .expect("Cannot increment latest version");
                    // This is a mock item, so pop an actual one for the IO to process.
                    continue;
                }
                _ => break action,
            }
        }
    }
}

#[async_trait]
impl IoSealCriteria for TestIO {
    async fn should_seal_l1_batch_unconditionally(
        &mut self,
        manager: &UpdatesManager,
    ) -> anyhow::Result<bool> {
        Ok((self.l1_batch_seal_fn)(manager))
    }

    fn should_seal_l2_block(&mut self, manager: &UpdatesManager) -> bool {
        (self.l2_block_seal_fn)(manager)
    }
}

#[async_trait]
impl StateKeeperIO for TestIO {
    fn chain_id(&self) -> L2ChainId {
        L2ChainId::default()
    }

    async fn initialize(&mut self) -> anyhow::Result<(IoCursor, Option<PendingBatchData>)> {
        let cursor = IoCursor {
            next_l2_block: self.l2_block_number,
            prev_l2_block_hash: H256::zero(),
            prev_l2_block_timestamp: self.timestamp.saturating_sub(1),
            l1_batch: self.batch_number,
        };
        let pending_batch = self.pending_batch.take();
        if pending_batch.is_some() {
            self.batch_number += 1;
        }
        Ok((cursor, pending_batch))
    }

    async fn wait_for_new_batch_params(
        &mut self,
        cursor: &IoCursor,
        _max_wait: Duration,
    ) -> anyhow::Result<Option<L1BatchParams>> {
        assert_eq!(cursor.next_l2_block, self.l2_block_number);
        assert_eq!(cursor.l1_batch, self.batch_number);

        let params = L1BatchParams {
            protocol_version: self.protocol_version,
            validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
            operator_address: self.fee_account,
            fee_input: self.fee_input,
            first_l2_block: L2BlockParams {
                timestamp: self.timestamp,
                virtual_blocks: 1,
            },
            pubdata_params: Default::default(),
        };
        self.l2_block_number += 1;
        self.timestamp += 1;
        self.batch_number += 1;
        Ok(Some(params))
    }

    async fn wait_for_new_l2_block_params(
        &mut self,
        cursor: &IoCursor,
        _max_wait: Duration,
    ) -> anyhow::Result<Option<L2BlockParams>> {
        assert_eq!(cursor.next_l2_block, self.l2_block_number);
        let params = L2BlockParams {
            timestamp: self.timestamp,
            // 1 is just a constant used for tests.
            virtual_blocks: 1,
        };
        self.l2_block_number += 1;
        self.timestamp += 1;
        Ok(Some(params))
    }

    fn update_next_l2_block_timestamp(&mut self, block_timestamp: &mut u64) {
        let action = self.pop_next_item("update_next_l2_block_timestamp");

        if let ScenarioItem::UpdateBlockTimestamp(_, timestamp) = action {
            *block_timestamp = timestamp;
        } else {
            // Return the action to the scenario.
            self.actions.lock().unwrap().push_front(action);
        }
    }

    async fn wait_for_next_tx(
        &mut self,
        max_wait: Duration,
        _l2_block_timestamp: u64,
    ) -> anyhow::Result<Option<Transaction>> {
        let action = self.pop_next_item("wait_for_next_tx");

        // Check whether we should ignore tx requests.
        if self.skipping_txs {
            // As per expectation, we should provide a delay given by the state keeper.
            tokio::time::sleep(max_wait).await;
            // Return the action to the scenario as we don't use it.
            self.actions.lock().unwrap().push_front(action);
            return Ok(None);
        }

        // We shouldn't, process normally.
        let ScenarioItem::Tx(_, tx, _) = action else {
            panic!("Unexpected action: {:?}", action);
        };
        Ok(Some(tx))
    }

    async fn rollback(&mut self, tx: Transaction) -> anyhow::Result<()> {
        let action = self.pop_next_item("rollback");
        let ScenarioItem::Rollback(_, expected_tx) = action else {
            panic!("Unexpected action: {:?}", action);
        };
        assert_eq!(
            tx, expected_tx,
            "Incorrect transaction has been rolled back"
        );
        self.skipping_txs = false;
        Ok(())
    }

    async fn reject(&mut self, tx: &Transaction, reason: UnexecutableReason) -> anyhow::Result<()> {
        let action = self.pop_next_item("reject");
        let ScenarioItem::Reject(_, expected_tx, expected_err) = action else {
            panic!("Unexpected action: {:?}", action);
        };
        assert_eq!(tx, &expected_tx, "Incorrect transaction has been rejected");
        assert_eq!(reason, expected_err);

        self.skipping_txs = false;
        Ok(())
    }

    async fn load_base_system_contracts(
        &self,
        _protocol_version: ProtocolVersionId,
        _cursor: &IoCursor,
    ) -> anyhow::Result<BaseSystemContracts> {
        Ok(BASE_SYSTEM_CONTRACTS.clone())
    }

    async fn load_batch_version_id(
        &self,
        _number: L1BatchNumber,
    ) -> anyhow::Result<ProtocolVersionId> {
        Ok(self.previous_batch_protocol_version)
    }

    async fn load_upgrade_tx(
        &self,
        version_id: ProtocolVersionId,
    ) -> anyhow::Result<Option<ProtocolUpgradeTx>> {
        Ok(self.protocol_upgrade_txs.get(&version_id).cloned())
    }

    async fn load_batch_state_hash(&self, _l1_batch_number: L1BatchNumber) -> anyhow::Result<H256> {
        Ok(H256::zero())
    }
}

/// Storage factory that produces empty VM storage for any batch.
///
/// Should only be used with a mock batch executor
/// that doesn't read from the storage. Prefer using `ConnectionPool` as a factory if it's available.
#[derive(Debug)]
pub struct MockReadStorageFactory;

#[async_trait]
impl ReadStorageFactory<OwnedStorage> for MockReadStorageFactory {
    async fn access_storage(
        &self,
        _stop_receiver: &watch::Receiver<bool>,
        _l1_batch_number: L1BatchNumber,
    ) -> Result<OwnedStorage, OrStopped> {
        let storage = InMemoryStorage::default();
        Ok(OwnedStorage::boxed(storage))
    }
}
