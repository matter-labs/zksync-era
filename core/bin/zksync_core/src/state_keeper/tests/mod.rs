use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

use crate::gas_tracker::constants::{
    BLOCK_COMMIT_BASE_COST, BLOCK_EXECUTE_BASE_COST, BLOCK_PROVE_BASE_COST,
};
use once_cell::sync::Lazy;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_config::constants::ZKPORTER_IS_AVAILABLE;
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes};
use zksync_types::{
    block::BlockGasCount, zk_evm::block_properties::BlockProperties, MiniblockNumber,
};
use zksync_utils::{h256_to_u256, time::millis_since_epoch};

use crate::state_keeper::{
    seal_criteria::{gas::GasCriterion, slots::SlotsCriterion, SealManager},
    types::ExecutionMetricsForCriteria,
};

use self::tester::{
    bootloader_tip_out_of_gas, pending_batch_data, random_tx, rejected_exec, successful_exec,
    TestScenario,
};

use super::keeper::POLL_WAIT_DURATION;

mod tester;

pub static BASE_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::load_from_disk);

pub fn default_block_properties() -> BlockProperties {
    BlockProperties {
        default_aa_code_hash: h256_to_u256(BASE_SYSTEM_CONTRACTS.default_aa.hash),
        zkporter_is_available: ZKPORTER_IS_AVAILABLE,
    }
}

#[test]
fn sealed_by_number_of_txs() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![Box::new(SlotsCriterion)],
        Box::new(|_| false),
        Box::new(|updates| updates.miniblock.executed_transactions.len() == 1),
    );

    let scenario = TestScenario::new();

    scenario
        .next_tx("First tx", random_tx(1), successful_exec())
        .miniblock_sealed("Miniblock 1")
        .next_tx("Second tx", random_tx(2), successful_exec())
        .miniblock_sealed("Miniblock 2")
        .batch_sealed("Batch 1")
        .run(sealer);
}

#[test]
fn sealed_by_gas() {
    let config = StateKeeperConfig {
        max_single_tx_gas: 62_002,
        reject_tx_at_gas_percentage: 1.0,
        close_block_at_gas_percentage: 0.5,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![Box::new(GasCriterion)],
        Box::new(|_| false),
        Box::new(|updates| updates.miniblock.executed_transactions.len() == 1),
    );

    let mut execution_result = successful_exec();
    let l1_gas_per_tx = BlockGasCount {
        commit: 1, // Both txs together with block_base_cost would bring it over the block 31_001 commit bound.
        prove: 0,
        execute: 0,
    };
    execution_result.add_tx_metrics(ExecutionMetricsForCriteria {
        l1_gas: l1_gas_per_tx,
        execution_metrics: Default::default(),
    });

    TestScenario::new()
        .next_tx("First tx", random_tx(1), execution_result.clone())
        .miniblock_sealed_with("Miniblock with a single tx", move |updates| {
            assert_eq!(
                updates.miniblock.l1_gas_count,
                l1_gas_per_tx,
                "L1 gas used by a miniblock should consist of the gas used by its txs"
            );
        })
        .next_tx("Second tx", random_tx(1), execution_result)
        .miniblock_sealed("Miniblock 2")
        .batch_sealed_with("Batch sealed with both txs", |_, updates, _| {
            assert_eq!(
                updates.l1_batch.l1_gas_count,
                BlockGasCount {
                    commit: BLOCK_COMMIT_BASE_COST + 2,
                    prove: BLOCK_PROVE_BASE_COST,
                    execute: BLOCK_EXECUTE_BASE_COST,
                },
                "L1 gas used by a batch should consists of gas used by its txs + basic block gas cost"
            );
        })
        .run(sealer);
}

#[test]
fn sealed_by_gas_then_by_num_tx() {
    let config = StateKeeperConfig {
        max_single_tx_gas: 62_000,
        reject_tx_at_gas_percentage: 1.0,
        close_block_at_gas_percentage: 0.5,
        transaction_slots: 3,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![Box::new(GasCriterion), Box::new(SlotsCriterion)],
        Box::new(|_| false),
        Box::new(|updates| updates.miniblock.executed_transactions.len() == 1),
    );

    let mut execution_result = successful_exec();
    execution_result.add_tx_metrics(ExecutionMetricsForCriteria {
        l1_gas: BlockGasCount {
            commit: 1,
            prove: 0,
            execute: 0,
        },
        execution_metrics: Default::default(),
    });

    // 1st tx is sealed by gas sealer; 2nd, 3rd, & 4th are sealed by slots sealer.
    TestScenario::new()
        .next_tx("First tx", random_tx(1), execution_result)
        .miniblock_sealed("Miniblock 1")
        .batch_sealed("Batch 1")
        .next_tx("Second tx", random_tx(2), successful_exec())
        .miniblock_sealed("Miniblock 2")
        .next_tx("Third tx", random_tx(3), successful_exec())
        .miniblock_sealed("Miniblock 3")
        .next_tx("Fourth tx", random_tx(4), successful_exec())
        .miniblock_sealed("Miniblock 4")
        .batch_sealed("Batch 2")
        .run(sealer);
}

#[test]
fn batch_sealed_before_miniblock_does() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![Box::new(SlotsCriterion)],
        Box::new(|_| false),
        Box::new(|updates| updates.miniblock.executed_transactions.len() == 3),
    );

    let scenario = TestScenario::new();

    // Miniblock sealer will not return true before the batch is sealed because the batch only has 2 txs.
    scenario
        .next_tx("First tx", random_tx(1), successful_exec())
        .next_tx("Second tx", random_tx(2), successful_exec())
        .miniblock_sealed_with("Miniblock with two txs", |updates| {
            assert_eq!(
                updates.miniblock.executed_transactions.len(),
                2,
                "The miniblock should have 2 txs"
            );
        })
        .batch_sealed("Batch 1")
        .run(sealer);
}

#[test]
fn basic_flow() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![Box::new(SlotsCriterion)],
        Box::new(|_| false),
        Box::new(|updates| updates.miniblock.executed_transactions.len() == 1),
    );

    TestScenario::new()
        .next_tx("First tx", random_tx(1), successful_exec())
        .miniblock_sealed("Miniblock 1")
        .next_tx("Second tx", random_tx(2), successful_exec())
        .miniblock_sealed("Miniblock 2")
        .batch_sealed("Batch 1")
        .run(sealer);
}

#[test]
fn rejected_tx() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![Box::new(SlotsCriterion)],
        Box::new(|_| false),
        Box::new(|updates| updates.miniblock.executed_transactions.len() == 1),
    );

    let rejected_tx = random_tx(1);
    TestScenario::new()
        .next_tx("Rejected tx", rejected_tx.clone(), rejected_exec())
        .tx_rejected("Tx got rejected", rejected_tx, None)
        .next_tx("Successful tx", random_tx(2), successful_exec())
        .miniblock_sealed("Miniblock with successful tx")
        .next_tx("Second successful tx", random_tx(3), successful_exec())
        .miniblock_sealed("Second miniblock")
        .batch_sealed("Batch with 2 successful txs")
        .run(sealer);
}

#[test]
fn bootloader_tip_out_of_gas_flow() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![Box::new(SlotsCriterion)],
        Box::new(|_| false),
        Box::new(|updates| updates.miniblock.executed_transactions.len() == 1),
    );

    let first_tx = random_tx(1);
    let bootloader_out_of_gas_tx = random_tx(2);
    let third_tx = random_tx(3);
    TestScenario::new()
        .next_tx("First tx", first_tx, successful_exec())
        .miniblock_sealed("Miniblock with 1st tx")
        .next_tx(
            "Tx -> Bootloader tip out of gas",
            bootloader_out_of_gas_tx.clone(),
            bootloader_tip_out_of_gas(),
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
        .miniblock_sealed("Miniblock with this tx sealed")
        .next_tx("Second tx of the 2nd batch", third_tx, successful_exec())
        .miniblock_sealed("Miniblock with 2nd tx")
        .batch_sealed("2nd batch sealed")
        .run(sealer);
}

#[test]
fn bootloader_config_has_been_updated() {
    let config = StateKeeperConfig {
        transaction_slots: 300,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![],
        SealManager::timeout_and_code_hash_batch_sealer(
            u64::MAX,
            BaseSystemContractsHashes {
                bootloader: Default::default(),
                default_aa: Default::default(),
            },
        ),
        Box::new(|_| false),
    );

    let pending_batch =
        pending_batch_data(vec![(MiniblockNumber(1), vec![random_tx(1), random_tx(2)])]);

    TestScenario::new()
        .load_pending_batch(pending_batch)
        .batch_sealed_with("Batch sealed with all 2 tx", |_, updates, _| {
            assert_eq!(
                updates.l1_batch.executed_transactions.len(),
                2,
                "There should be 2 transactions in the batch"
            );
        })
        .next_tx("Final tx of batch", random_tx(3), successful_exec())
        .miniblock_sealed("Miniblock with this tx sealed")
        .batch_sealed_with("Batch sealed with all 1 tx", |_, updates, _| {
            assert_eq!(
                updates.l1_batch.executed_transactions.len(),
                1,
                "There should be 1 transactions in the batch"
            );
        })
        .run(sealer);
}

#[test]
fn pending_batch_is_applied() {
    let config = StateKeeperConfig {
        transaction_slots: 3,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![Box::new(SlotsCriterion)],
        Box::new(|_| false),
        Box::new(|updates| updates.miniblock.executed_transactions.len() == 1),
    );

    let pending_batch = pending_batch_data(vec![
        (MiniblockNumber(1), vec![random_tx(1)]),
        (MiniblockNumber(2), vec![random_tx(2)]),
    ]);

    // We configured state keeper to use different system contract hashes, so it must seal the pending batch immediately.
    TestScenario::new()
        .load_pending_batch(pending_batch)
        .next_tx("Final tx of batch", random_tx(3), successful_exec())
        .miniblock_sealed_with("Miniblock with a single tx", |updates| {
            assert_eq!(
                updates.miniblock.executed_transactions.len(),
                1,
                "Only one transaction should be in miniblock"
            );
        })
        .batch_sealed_with("Batch sealed with all 3 txs", |_, updates, _| {
            assert_eq!(
                updates.l1_batch.executed_transactions.len(),
                3,
                "There should be 3 transactions in the batch"
            );
        })
        .run(sealer);
}

/// Unconditionally seal the batch without triggering specific criteria.
#[test]
fn unconditional_sealing() {
    // Trigger to know when to seal the batch.
    // Once miniblock with one tx would be sealed, trigger would allow batch to be sealed as well.
    let batch_seal_trigger = Arc::new(AtomicBool::new(false));
    let batch_seal_trigger_checker = batch_seal_trigger.clone();
    let start = Instant::now();
    let seal_miniblock_after = POLL_WAIT_DURATION; // Seal after 2 state keeper polling duration intervals.

    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![Box::new(SlotsCriterion)],
        Box::new(move |_| batch_seal_trigger_checker.load(Ordering::Relaxed)),
        Box::new(move |upd_manager| {
            if upd_manager.pending_executed_transactions_len() != 0
                && start.elapsed() >= seal_miniblock_after
            {
                batch_seal_trigger.store(true, Ordering::Relaxed);
                true
            } else {
                false
            }
        }),
    );

    TestScenario::new()
        .next_tx("The only tx", random_tx(1), successful_exec())
        .no_txs_until_next_action("We don't give transaction to wait for miniblock to be sealed")
        .miniblock_sealed("Miniblock is sealed with just one tx")
        .no_txs_until_next_action("Still no tx")
        .batch_sealed("Batch is sealed with just one tx")
        .run(sealer);
}

/// Checks the next miniblock sealed after pending batch has a correct timestamp
#[test]
fn miniblock_timestamp_after_pending_batch() {
    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![Box::new(SlotsCriterion)],
        Box::new(|_| false),
        Box::new(|updates| updates.miniblock.executed_transactions.len() == 1),
    );

    let pending_batch = pending_batch_data(vec![(MiniblockNumber(1), vec![random_tx(1)])]);

    let current_timestamp = (millis_since_epoch() / 1000) as u64;

    TestScenario::new()
        .load_pending_batch(pending_batch)
        .next_tx(
            "First tx after pending batch",
            random_tx(2),
            successful_exec(),
        )
        .miniblock_sealed_with("Miniblock with a single tx", move |updates| {
            assert!(
                updates.miniblock.timestamp >= current_timestamp,
                "Timestamp cannot decrease"
            );
        })
        .batch_sealed("Batch is sealed with two transactions")
        .run(sealer);
}

/// Makes sure that the timestamp doesn't decrease in consequent miniblocks.
///
/// Timestamps are faked in the IO layer, so this test mostly makes sure that the state keeper doesn't substitute
/// any unexpected value on its own.
#[test]
fn time_is_monotonic() {
    let timestamp_first_miniblock = Arc::new(AtomicU64::new(0u64)); // Time is faked in tests.
    let timestamp_second_miniblock = timestamp_first_miniblock.clone();
    let timestamp_third_miniblock = timestamp_first_miniblock.clone();

    let config = StateKeeperConfig {
        transaction_slots: 2,
        ..Default::default()
    };
    let sealer = SealManager::custom(
        config,
        vec![Box::new(SlotsCriterion)],
        Box::new(|_| false),
        Box::new(|updates| updates.miniblock.executed_transactions.len() == 1),
    );

    let scenario = TestScenario::new();

    scenario
        .next_tx("First tx", random_tx(1), successful_exec())
        .miniblock_sealed_with("Miniblock 1", move |updates| {
            let min_expected = timestamp_first_miniblock.load(Ordering::Relaxed);
            let actual = updates.miniblock.timestamp;
            assert!(
                actual > min_expected,
                "First miniblock: Timestamp cannot decrease. Expected at least {}, got {}",
                min_expected,
                actual
            );
            timestamp_first_miniblock.store(updates.miniblock.timestamp, Ordering::Relaxed);
        })
        .next_tx("Second tx", random_tx(2), successful_exec())
        .miniblock_sealed_with("Miniblock 2", move |updates| {
            let min_expected = timestamp_second_miniblock.load(Ordering::Relaxed);
            let actual = updates.miniblock.timestamp;
            assert!(
                actual > min_expected,
                "Second miniblock: Timestamp cannot decrease. Expected at least {}, got {}",
                min_expected,
                actual
            );
            timestamp_second_miniblock.store(updates.miniblock.timestamp, Ordering::Relaxed);
        })
        .batch_sealed_with("Batch 1", move |_, updates, _| {
            // Timestamp from the currently stored miniblock would be used in the fictive miniblock.
            // It should be correct as well.
            let min_expected = timestamp_third_miniblock.load(Ordering::Relaxed);
            let actual = updates.miniblock.timestamp;
            assert!(
                actual > min_expected,
                "Fictive miniblock: Timestamp cannot decrease. Expected at least {}, got {}",
                min_expected,
                actual
            );
            timestamp_third_miniblock.store(updates.miniblock.timestamp, Ordering::Relaxed);
        })
        .run(sealer);
}
