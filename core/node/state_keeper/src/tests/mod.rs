use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

use tokio::sync::watch;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_multivm::{
    interface::{
        ExecutionResult, Halt, L1BatchEnv, L2BlockEnv, Refunds, SystemEnv, TxExecutionMode,
        VmExecutionLogs, VmExecutionMetrics, VmExecutionResultAndLogs, VmExecutionStatistics,
    },
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};
use zksync_node_test_utils::create_l2_transaction;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    block::{BlockGasCount, L2BlockExecutionData, L2BlockHasher},
    fee_model::{BatchFeeInput, PubdataIndependentBatchFeeModelInput},
    AccountTreeId, Address, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId, StorageKey,
    StorageLog, StorageLogKind, StorageLogWithPreviousValue, Transaction, H256, U256,
    ZKPORTER_IS_AVAILABLE,
};
use zksync_utils::u256_to_h256;

use crate::{
    batch_executor::TxExecutionResult,
    io::PendingBatchData,
    keeper::POLL_WAIT_DURATION,
    seal_criteria::{
        criteria::{GasCriterion, SlotsCriterion},
        SequencerSealer, UnexecutableReason,
    },
    testonly::{
        successful_exec,
        test_batch_executor::{
            random_tx, random_upgrade_tx, rejected_exec, successful_exec_with_metrics,
            MockReadStorageFactory, TestBatchExecutorBuilder, TestIO, TestScenario, FEE_ACCOUNT,
        },
        BASE_SYSTEM_CONTRACTS,
    },
    types::ExecutionMetricsForCriteria,
    updates::UpdatesManager,
    utils::l1_batch_base_cost,
    ZkSyncStateKeeper,
};

/// Creates a mock `PendingBatchData` object containing the provided sequence of L2 blocks.
pub(crate) fn pending_batch_data(pending_l2_blocks: Vec<L2BlockExecutionData>) -> PendingBatchData {
    PendingBatchData {
        l1_batch_env: default_l1_batch_env(1, 1, FEE_ACCOUNT),
        system_env: SystemEnv {
            zk_porter_available: false,
            version: ProtocolVersionId::latest(),
            base_system_smart_contracts: BASE_SYSTEM_CONTRACTS.clone(),
            bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
            execution_mode: TxExecutionMode::VerifyExecute,
            default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
            chain_id: L2ChainId::from(270),
        },
        pending_l2_blocks,
    }
}

pub(super) fn default_system_env() -> SystemEnv {
    SystemEnv {
        zk_porter_available: ZKPORTER_IS_AVAILABLE,
        version: ProtocolVersionId::latest(),
        base_system_smart_contracts: BASE_SYSTEM_CONTRACTS.clone(),
        bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        execution_mode: TxExecutionMode::VerifyExecute,
        default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        chain_id: L2ChainId::from(270),
    }
}

pub(super) fn default_l1_batch_env(
    number: u32,
    timestamp: u64,
    fee_account: Address,
) -> L1BatchEnv {
    L1BatchEnv {
        previous_batch_hash: None,
        number: L1BatchNumber(number),
        timestamp,
        fee_account,
        enforced_base_fee: None,
        first_l2_block: L2BlockEnv {
            number,
            timestamp,
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(number - 1)),
            max_virtual_blocks_to_create: 1,
        },
        fee_input: BatchFeeInput::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
            fair_l2_gas_price: 1,
            fair_pubdata_price: 1,
            l1_gas_price: 1,
        }),
    }
}

pub(super) fn create_updates_manager() -> UpdatesManager {
    let l1_batch_env = default_l1_batch_env(1, 1, Address::default());
    UpdatesManager::new(&l1_batch_env, &default_system_env())
}

pub(super) fn create_transaction(fee_per_gas: u64, gas_per_pubdata: u64) -> Transaction {
    create_l2_transaction(fee_per_gas, gas_per_pubdata).into()
}

pub(super) fn create_execution_result(
    storage_logs: impl IntoIterator<Item = (U256, Query)>,
) -> VmExecutionResultAndLogs {
    let storage_logs: Vec<_> = storage_logs
        .into_iter()
        .map(|(key, query)| query.into_log(key))
        .collect();

    let total_log_queries = storage_logs.len() + 2;
    VmExecutionResultAndLogs {
        result: ExecutionResult::Success { output: vec![] },
        logs: VmExecutionLogs {
            events: vec![],
            system_l2_to_l1_logs: vec![],
            user_l2_to_l1_logs: vec![],
            storage_logs,
            total_log_queries_count: total_log_queries,
        },
        statistics: VmExecutionStatistics {
            contracts_used: 0,
            cycles_used: 0,
            gas_used: 0,
            gas_remaining: 0,
            computational_gas_used: 0,
            total_log_queries,
            pubdata_published: 0,
            circuit_statistic: Default::default(),
        },
        refunds: Refunds::default(),
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Query {
    Read(U256),
    InitialWrite(U256),
    RepeatedWrite(U256, U256),
}

impl Query {
    fn into_log(self, key: U256) -> StorageLogWithPreviousValue {
        StorageLogWithPreviousValue {
            log: StorageLog {
                kind: match self {
                    Self::Read(_) => StorageLogKind::Read,
                    Self::InitialWrite(_) => StorageLogKind::InitialWrite,
                    Self::RepeatedWrite(_, _) => StorageLogKind::RepeatedWrite,
                },
                key: StorageKey::new(AccountTreeId::new(Address::default()), u256_to_h256(key)),
                value: u256_to_h256(match self {
                    Query::Read(_) => U256::zero(),
                    Query::InitialWrite(value) | Query::RepeatedWrite(_, value) => value,
                }),
            },
            previous_value: u256_to_h256(match self {
                Query::Read(value) | Query::RepeatedWrite(value, _) => value,
                Query::InitialWrite(_) => U256::zero(),
            }),
        }
    }
}

#[tokio::test]
async fn sealed_by_number_of_txs() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..StateKeeperConfig::default()
    };
    let sealer = SequencerSealer::with_sealers(config, vec![Box::new(SlotsCriterion)]);

    TestScenario::new()
        .seal_l2_block_when(|updates| updates.l2_block.executed_transactions.len() == 1)
        .next_tx("First tx", random_tx(1), successful_exec())
        .l2_block_sealed("L2 block 1")
        .next_tx("Second tx", random_tx(2), successful_exec())
        .l2_block_sealed("L2 block 2")
        .batch_sealed("Batch 1")
        .run(sealer)
        .await;
}

#[tokio::test]
async fn sealed_by_gas() {
    let config = StateKeeperConfig {
        max_single_tx_gas: 62_002,
        reject_tx_at_gas_percentage: 1.0,
        close_block_at_gas_percentage: 0.5,
        ..StateKeeperConfig::default()
    };
    let sealer = SequencerSealer::with_sealers(config, vec![Box::new(GasCriterion)]);

    let l1_gas_per_tx = BlockGasCount {
        commit: 1, // Both txs together with `block_base_cost` would bring it over the block `31_001` commit bound.
        prove: 0,
        execute: 0,
    };
    let execution_result = successful_exec_with_metrics(ExecutionMetricsForCriteria {
        l1_gas: l1_gas_per_tx,
        execution_metrics: VmExecutionMetrics::default(),
    });

    TestScenario::new()
        .seal_l2_block_when(|updates| {
            updates.l2_block.executed_transactions.len() == 1
        })
        .next_tx("First tx", random_tx(1), execution_result.clone())
        .l2_block_sealed_with("L2 block with a single tx", move |updates| {
            assert_eq!(
                updates.l2_block.l1_gas_count,
                l1_gas_per_tx,
                "L1 gas used by a L2 block should consist of the gas used by its txs"
            );
        })
        .next_tx("Second tx", random_tx(1), execution_result)
        .l2_block_sealed("L2 block 2")
        .batch_sealed_with("Batch sealed with both txs", |updates| {
            assert_eq!(
                updates.l1_batch.l1_gas_count,
                BlockGasCount {
                    commit: l1_batch_base_cost(AggregatedActionType::Commit) + 2,
                    prove: l1_batch_base_cost(AggregatedActionType::PublishProofOnchain),
                    execute: l1_batch_base_cost(AggregatedActionType::Execute),
                },
                "L1 gas used by a batch should consist of gas used by its txs + basic block gas cost"
            );
        })
        .run(sealer).await;
}

#[tokio::test]
async fn sealed_by_gas_then_by_num_tx() {
    let config = StateKeeperConfig {
        max_single_tx_gas: 62_000,
        reject_tx_at_gas_percentage: 1.0,
        close_block_at_gas_percentage: 0.5,
        transaction_slots: 3,
        ..StateKeeperConfig::default()
    };
    let sealer = SequencerSealer::with_sealers(
        config,
        vec![Box::new(GasCriterion), Box::new(SlotsCriterion)],
    );

    let execution_result = successful_exec_with_metrics(ExecutionMetricsForCriteria {
        l1_gas: BlockGasCount {
            commit: 1,
            prove: 0,
            execute: 0,
        },
        execution_metrics: VmExecutionMetrics::default(),
    });

    // 1st tx is sealed by gas sealer; 2nd, 3rd, & 4th are sealed by slots sealer.
    TestScenario::new()
        .seal_l2_block_when(|updates| updates.l2_block.executed_transactions.len() == 1)
        .next_tx("First tx", random_tx(1), execution_result)
        .l2_block_sealed("L2 block 1")
        .batch_sealed("Batch 1")
        .next_tx("Second tx", random_tx(2), successful_exec())
        .l2_block_sealed("L2 block 2")
        .next_tx("Third tx", random_tx(3), successful_exec())
        .l2_block_sealed("L2 block 3")
        .next_tx("Fourth tx", random_tx(4), successful_exec())
        .l2_block_sealed("L2 block 4")
        .batch_sealed("Batch 2")
        .run(sealer)
        .await;
}

#[tokio::test]
async fn batch_sealed_before_l2_block_does() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..StateKeeperConfig::default()
    };
    let sealer = SequencerSealer::with_sealers(config, vec![Box::new(SlotsCriterion)]);

    // L2 block sealer will not return true before the batch is sealed because the batch only has 2 txs.
    TestScenario::new()
        .seal_l2_block_when(|updates| updates.l2_block.executed_transactions.len() == 3)
        .next_tx("First tx", random_tx(1), successful_exec())
        .next_tx("Second tx", random_tx(2), successful_exec())
        .l2_block_sealed_with("L2 block with two txs", |updates| {
            assert_eq!(
                updates.l2_block.executed_transactions.len(),
                2,
                "The L2 block should have 2 txs"
            );
        })
        .batch_sealed("Batch 1")
        .run(sealer)
        .await;
}

#[tokio::test]
async fn rejected_tx() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..StateKeeperConfig::default()
    };
    let sealer = SequencerSealer::with_sealers(config, vec![Box::new(SlotsCriterion)]);

    let rejected_tx = random_tx(1);
    TestScenario::new()
        .seal_l2_block_when(|updates| updates.l2_block.executed_transactions.len() == 1)
        .next_tx("Rejected tx", rejected_tx.clone(), rejected_exec())
        .tx_rejected(
            "Tx got rejected",
            rejected_tx,
            UnexecutableReason::Halt(Halt::InnerTxError),
        )
        .next_tx("Successful tx", random_tx(2), successful_exec())
        .l2_block_sealed("L2 block with successful tx")
        .next_tx("Second successful tx", random_tx(3), successful_exec())
        .l2_block_sealed("Second L2 block")
        .batch_sealed("Batch with 2 successful txs")
        .run(sealer)
        .await;
}

#[tokio::test]
async fn bootloader_tip_out_of_gas_flow() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..StateKeeperConfig::default()
    };
    let sealer = SequencerSealer::with_sealers(config, vec![Box::new(SlotsCriterion)]);

    let first_tx = random_tx(1);
    let bootloader_out_of_gas_tx = random_tx(2);
    let third_tx = random_tx(3);
    TestScenario::new()
        .seal_l2_block_when(|updates| updates.l2_block.executed_transactions.len() == 1)
        .next_tx("First tx", first_tx, successful_exec())
        .l2_block_sealed("L2 block with 1st tx")
        .next_tx(
            "Tx -> Bootloader tip out of gas",
            bootloader_out_of_gas_tx.clone(),
            TxExecutionResult::BootloaderOutOfGasForTx,
        )
        .tx_rollback(
            "Last tx rolled back to seal the block",
            bootloader_out_of_gas_tx.clone(),
        )
        .batch_sealed("Batch sealed with 1 tx")
        .next_tx(
            "Same tx now succeeds",
            bootloader_out_of_gas_tx,
            successful_exec(),
        )
        .l2_block_sealed("L2 block with this tx sealed")
        .next_tx("Second tx of the 2nd batch", third_tx, successful_exec())
        .l2_block_sealed("L2 block with 2nd tx")
        .batch_sealed("2nd batch sealed")
        .run(sealer)
        .await;
}

#[tokio::test]
async fn pending_batch_is_applied() {
    let config = StateKeeperConfig {
        transaction_slots: 3,
        ..StateKeeperConfig::default()
    };
    let sealer = SequencerSealer::with_sealers(config, vec![Box::new(SlotsCriterion)]);

    let pending_batch = pending_batch_data(vec![
        L2BlockExecutionData {
            number: L2BlockNumber(1),
            timestamp: 1,
            prev_block_hash: L2BlockHasher::new(L2BlockNumber(0), 0, H256::zero())
                .finalize(ProtocolVersionId::latest()),
            virtual_blocks: 1,
            txs: vec![random_tx(1)],
        },
        L2BlockExecutionData {
            number: L2BlockNumber(2),
            timestamp: 2,
            prev_block_hash: L2BlockHasher::new(L2BlockNumber(1), 1, H256::zero())
                .finalize(ProtocolVersionId::latest()),
            virtual_blocks: 1,
            txs: vec![random_tx(2)],
        },
    ]);

    // We configured state keeper to use different system contract hashes, so it must seal the pending batch immediately.
    TestScenario::new()
        .seal_l2_block_when(|updates| updates.l2_block.executed_transactions.len() == 1)
        .load_pending_batch(pending_batch)
        .next_tx("Final tx of batch", random_tx(3), successful_exec())
        .l2_block_sealed_with("L2 block with a single tx", |updates| {
            assert_eq!(
                updates.l2_block.executed_transactions.len(),
                1,
                "Only one transaction should be in L2 block"
            );
        })
        .batch_sealed_with("Batch sealed with all 3 txs", |updates| {
            assert_eq!(
                updates.l1_batch.executed_transactions.len(),
                3,
                "There should be 3 transactions in the batch"
            );
        })
        .run(sealer)
        .await;
}

/// Load protocol upgrade transactions
#[tokio::test]
async fn load_upgrade_tx() {
    let sealer = SequencerSealer::default();
    let scenario = TestScenario::new();
    let batch_executor_base = TestBatchExecutorBuilder::new(&scenario);
    let (stop_sender, stop_receiver) = watch::channel(false);

    let (mut io, output_handler) = TestIO::new(stop_sender, scenario);
    io.add_upgrade_tx(ProtocolVersionId::latest(), random_upgrade_tx(1));
    io.add_upgrade_tx(ProtocolVersionId::next(), random_upgrade_tx(2));

    let mut sk = ZkSyncStateKeeper::new(
        stop_receiver,
        Box::new(io),
        Box::new(batch_executor_base),
        output_handler,
        Arc::new(sealer),
        Arc::<MockReadStorageFactory>::default(),
    );

    // Since the version hasn't changed, and we are not using shared bridge, we should not load any
    // upgrade transactions.
    assert_eq!(
        sk.load_protocol_upgrade_tx(&[], ProtocolVersionId::latest(), L1BatchNumber(2))
            .await
            .unwrap(),
        None
    );

    // If the protocol version has changed, we should load the upgrade transaction.
    assert_eq!(
        sk.load_protocol_upgrade_tx(&[], ProtocolVersionId::next(), L1BatchNumber(2))
            .await
            .unwrap(),
        Some(random_upgrade_tx(2))
    );

    // TODO: add one more test case for the shared bridge after it's integrated.
    // If we are processing the 1st batch while using the shared bridge,
    // we should load the upgrade transaction -- that's the `SetChainIdUpgrade`.
}

/// Unconditionally seal the batch without triggering specific criteria.
/// TODO(PLA-881): this test can be flaky if run under load.
#[tokio::test]
async fn unconditional_sealing() {
    // Trigger to know when to seal the batch.
    // Once L2 block with one tx would be sealed, trigger would allow batch to be sealed as well.
    let batch_seal_trigger = Arc::new(AtomicBool::new(false));
    let batch_seal_trigger_checker = batch_seal_trigger.clone();
    let start = Instant::now();
    let seal_l2_block_after = POLL_WAIT_DURATION; // Seal after 2 state keeper polling duration intervals.

    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..StateKeeperConfig::default()
    };
    let sealer = SequencerSealer::with_sealers(config, vec![Box::new(SlotsCriterion)]);

    TestScenario::new()
        .seal_l1_batch_when(move |_| batch_seal_trigger_checker.load(Ordering::Relaxed))
        .seal_l2_block_when(move |manager| {
            if manager.pending_executed_transactions_len() != 0
                && start.elapsed() >= seal_l2_block_after
            {
                batch_seal_trigger.store(true, Ordering::Relaxed);
                true
            } else {
                false
            }
        })
        .next_tx("The only tx", random_tx(1), successful_exec())
        .no_txs_until_next_action("We don't give transaction to wait for L2 block to be sealed")
        .l2_block_sealed("L2 block is sealed with just one tx")
        .no_txs_until_next_action("Still no tx")
        .batch_sealed("Batch is sealed with just one tx")
        .run(sealer)
        .await;
}

/// Checks the next L2 block sealed after pending batch has a correct timestamp
#[tokio::test]
async fn l2_block_timestamp_after_pending_batch() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..StateKeeperConfig::default()
    };
    let sealer = SequencerSealer::with_sealers(config, vec![Box::new(SlotsCriterion)]);

    let pending_batch = pending_batch_data(vec![L2BlockExecutionData {
        number: L2BlockNumber(1),
        timestamp: 1,
        prev_block_hash: L2BlockHasher::new(L2BlockNumber(0), 0, H256::zero())
            .finalize(ProtocolVersionId::latest()),
        virtual_blocks: 1,
        txs: vec![random_tx(1)],
    }]);

    TestScenario::new()
        .seal_l2_block_when(|updates| updates.l2_block.executed_transactions.len() == 1)
        .load_pending_batch(pending_batch)
        .next_tx(
            "First tx after pending batch",
            random_tx(2),
            successful_exec(),
        )
        .l2_block_sealed_with("L2 block with a single tx", move |updates| {
            assert_eq!(
                updates.l2_block.timestamp, 2,
                "Timestamp for the new block must be taken from the test IO"
            );
        })
        .batch_sealed("Batch is sealed with two transactions")
        .run(sealer)
        .await;
}

/// Makes sure that the timestamp doesn't decrease in consequent L2 blocks.
///
/// Timestamps are faked in the IO layer, so this test mostly makes sure that the state keeper doesn't substitute
/// any unexpected value on its own.
#[tokio::test]
async fn time_is_monotonic() {
    let timestamp_first_l2_block = Arc::new(AtomicU64::new(0u64)); // Time is faked in tests.
    let timestamp_second_l2_block = timestamp_first_l2_block.clone();
    let timestamp_third_l2_block = timestamp_first_l2_block.clone();

    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..StateKeeperConfig::default()
    };
    let sealer = SequencerSealer::with_sealers(config, vec![Box::new(SlotsCriterion)]);

    TestScenario::new()
        .seal_l2_block_when(|updates| updates.l2_block.executed_transactions.len() == 1)
        .next_tx("First tx", random_tx(1), successful_exec())
        .l2_block_sealed_with("L2 block 1", move |updates| {
            let min_expected = timestamp_first_l2_block.load(Ordering::Relaxed);
            let actual = updates.l2_block.timestamp;
            assert!(
                actual > min_expected,
                "First L2 block: Timestamp cannot decrease. Expected at least {}, got {}",
                min_expected,
                actual
            );
            timestamp_first_l2_block.store(updates.l2_block.timestamp, Ordering::Relaxed);
        })
        .next_tx("Second tx", random_tx(2), successful_exec())
        .l2_block_sealed_with("L2 block 2", move |updates| {
            let min_expected = timestamp_second_l2_block.load(Ordering::Relaxed);
            let actual = updates.l2_block.timestamp;
            assert!(
                actual > min_expected,
                "Second L2 block: Timestamp cannot decrease. Expected at least {}, got {}",
                min_expected,
                actual
            );
            timestamp_second_l2_block.store(updates.l2_block.timestamp, Ordering::Relaxed);
        })
        .batch_sealed_with("Batch 1", move |updates| {
            // Timestamp from the currently stored L2 block would be used in the fictive L2 block.
            // It should be correct as well.
            let min_expected = timestamp_third_l2_block.load(Ordering::Relaxed);
            let actual = updates.l2_block.timestamp;
            assert!(
                actual > min_expected,
                "Fictive L2 block: Timestamp cannot decrease. Expected at least {}, got {}",
                min_expected,
                actual
            );
            timestamp_third_l2_block.store(updates.l2_block.timestamp, Ordering::Relaxed);
        })
        .run(sealer)
        .await;
}

#[tokio::test]
async fn protocol_upgrade() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..StateKeeperConfig::default()
    };
    let sealer = SequencerSealer::with_sealers(config, vec![Box::new(SlotsCriterion)]);

    TestScenario::new()
        .seal_l2_block_when(|updates| updates.l2_block.executed_transactions.len() == 1)
        .next_tx("First tx", random_tx(1), successful_exec())
        .l2_block_sealed("L2 block 1")
        .increment_protocol_version("Increment protocol version")
        .next_tx("Second tx", random_tx(2), successful_exec())
        .l2_block_sealed("L2 block 2")
        .batch_sealed_with("Batch 1", move |updates| {
            assert_eq!(
                updates.protocol_version(),
                ProtocolVersionId::latest(),
                "Should close batch with initial protocol version"
            )
        })
        .next_tx("Third tx", random_tx(3), successful_exec())
        .l2_block_sealed_with("L2 block 3", move |updates| {
            assert_eq!(
                updates.protocol_version(),
                ProtocolVersionId::next(),
                "Should open batch with current protocol version"
            )
        })
        .next_tx("Fourth tx", random_tx(4), successful_exec())
        .l2_block_sealed("L2 block 4")
        .batch_sealed("Batch 2")
        .run(sealer)
        .await;
}
