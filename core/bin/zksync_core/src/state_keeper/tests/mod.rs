use zksync_config::configs::chain::StateKeeperConfig;
use zksync_types::MiniblockNumber;

use crate::state_keeper::seal_criteria::{slots::SlotsCriterion, SealManager};

use self::tester::{
    bootloader_tip_out_of_gas, pending_batch_data, random_tx, rejected_exec, successful_exec,
    TestScenario,
};

mod tester;

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
