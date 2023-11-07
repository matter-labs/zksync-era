//!
//! Tests for the bootloader
//! The description for each of the tests can be found in the corresponding `.yul` file.
//!

use crate::interface::{
    ExecutionResult, Halt, L2BlockEnv, TxExecutionMode, VmExecutionMode, VmInterface,
};
use crate::vm_latest::constants::{
    BOOTLOADER_HEAP_PAGE, TX_OPERATOR_L2_BLOCK_INFO_OFFSET, TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO,
};
use crate::vm_latest::tests::tester::default_l1_batch;
use crate::vm_latest::tests::tester::VmTesterBuilder;
use crate::vm_latest::utils::l2_blocks::get_l2_block_hash_key;
use crate::vm_latest::{HistoryEnabled, Vm};
use crate::HistoryMode;
use zk_evm_1_4_0::aux_structures::Timestamp;
use zksync_state::WriteStorage;
use zksync_system_constants::REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE;
use zksync_types::block::pack_block_info;
use zksync_types::{
    block::{legacy_miniblock_hash, miniblock_hash},
    AccountTreeId, Execute, ExecuteTransactionCommon, L1BatchNumber, L1TxCommonData,
    MiniblockNumber, StorageKey, Transaction, H160, H256, SYSTEM_CONTEXT_ADDRESS,
    SYSTEM_CONTEXT_BLOCK_INFO_POSITION, SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION, U256,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

fn get_l1_noop() -> Transaction {
    Transaction {
        common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
            sender: H160::random(),
            gas_limit: U256::from(2000000u32),
            gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
            ..Default::default()
        }),
        execute: Execute {
            contract_address: H160::zero(),
            calldata: vec![],
            value: U256::zero(),
            factory_deps: None,
        },
        received_timestamp_ms: 0,
        raw_bytes: None,
    }
}

#[test]
fn test_l2_block_initialization_timestamp() {
    // This test checks that the L2 block initialization works correctly.
    // Here we check that that the first block must have timestamp that is greater or equal to the timestamp
    // of the current batch.

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    // Override the timestamp of the current miniblock to be 0.
    vm.vm.bootloader_state.push_l2_block(L2BlockEnv {
        number: 1,
        timestamp: 0,
        prev_block_hash: legacy_miniblock_hash(MiniblockNumber(0)),
        max_virtual_blocks_to_create: 1,
    });
    let l1_tx = get_l1_noop();

    vm.vm.push_transaction(l1_tx);
    let res = vm.vm.execute(VmExecutionMode::OneTx);

    assert_eq!(
        res.result,
        ExecutionResult::Halt {reason: Halt::FailedToSetL2Block("The timestamp of the L2 block must be greater than or equal to the timestamp of the current batch".to_string())}
    );
}

#[test]
fn test_l2_block_initialization_number_non_zero() {
    // This test checks that the L2 block initialization works correctly.
    // Here we check that the first miniblock number can not be zero.

    let l1_batch = default_l1_batch(L1BatchNumber(1));
    let first_l2_block = L2BlockEnv {
        number: 0,
        timestamp: l1_batch.timestamp,
        prev_block_hash: legacy_miniblock_hash(MiniblockNumber(0)),
        max_virtual_blocks_to_create: 1,
    };

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_l1_batch_env(l1_batch)
        .with_random_rich_accounts(1)
        .build();

    let l1_tx = get_l1_noop();

    vm.vm.push_transaction(l1_tx);

    let timestamp = Timestamp(vm.vm.state.local_state.timestamp);
    set_manual_l2_block_info(&mut vm.vm, 0, first_l2_block, timestamp);

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    assert_eq!(
        res.result,
        ExecutionResult::Halt {
            reason: Halt::FailedToSetL2Block(
                "L2 block number is never expected to be zero".to_string()
            )
        }
    );
}

fn test_same_l2_block(
    expected_error: Option<Halt>,
    override_timestamp: Option<u64>,
    override_prev_block_hash: Option<H256>,
) {
    let mut l1_batch = default_l1_batch(L1BatchNumber(1));
    l1_batch.timestamp = 1;
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_l1_batch_env(l1_batch)
        .with_random_rich_accounts(1)
        .build();

    let l1_tx = get_l1_noop();
    vm.vm.push_transaction(l1_tx.clone());
    let res = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!res.result.is_failed());

    let mut current_l2_block = vm.vm.batch_env.first_l2_block;

    if let Some(timestamp) = override_timestamp {
        current_l2_block.timestamp = timestamp;
    }
    if let Some(prev_block_hash) = override_prev_block_hash {
        current_l2_block.prev_block_hash = prev_block_hash;
    }

    if (None, None) == (override_timestamp, override_prev_block_hash) {
        current_l2_block.max_virtual_blocks_to_create = 0;
    }

    vm.vm.push_transaction(l1_tx);
    let timestamp = Timestamp(vm.vm.state.local_state.timestamp);
    set_manual_l2_block_info(&mut vm.vm, 1, current_l2_block, timestamp);

    let result = vm.vm.execute(VmExecutionMode::OneTx);

    if let Some(err) = expected_error {
        assert_eq!(result.result, ExecutionResult::Halt { reason: err });
    } else {
        assert_eq!(result.result, ExecutionResult::Success { output: vec![] });
    }
}

#[test]
fn test_l2_block_same_l2_block() {
    // This test aims to test the case when there are multiple transactions inside the same L2 block.

    // Case 1: Incorrect timestamp
    test_same_l2_block(
        Some(Halt::FailedToSetL2Block(
            "The timestamp of the same L2 block must be same".to_string(),
        )),
        Some(0),
        None,
    );

    // Case 2: Incorrect previous block hash
    test_same_l2_block(
        Some(Halt::FailedToSetL2Block(
            "The previous hash of the same L2 block must be same".to_string(),
        )),
        None,
        Some(H256::zero()),
    );

    // Case 3: Correct continuation of the same L2 block
    test_same_l2_block(None, None, None);
}

fn test_new_l2_block(
    first_l2_block: L2BlockEnv,
    overriden_second_block_number: Option<u32>,
    overriden_second_block_timestamp: Option<u64>,
    overriden_second_block_prev_block_hash: Option<H256>,
    expected_error: Option<Halt>,
) {
    let mut l1_batch = default_l1_batch(L1BatchNumber(1));
    l1_batch.timestamp = 1;
    l1_batch.first_l2_block = first_l2_block;

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_l1_batch_env(l1_batch)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let l1_tx = get_l1_noop();

    // Firstly we execute the first transaction
    vm.vm.push_transaction(l1_tx.clone());
    vm.vm.execute(VmExecutionMode::OneTx);

    let mut second_l2_block = vm.vm.batch_env.first_l2_block;
    second_l2_block.number += 1;
    second_l2_block.timestamp += 1;
    second_l2_block.prev_block_hash = vm.vm.bootloader_state.last_l2_block().get_hash();

    if let Some(block_number) = overriden_second_block_number {
        second_l2_block.number = block_number;
    }
    if let Some(timestamp) = overriden_second_block_timestamp {
        second_l2_block.timestamp = timestamp;
    }
    if let Some(prev_block_hash) = overriden_second_block_prev_block_hash {
        second_l2_block.prev_block_hash = prev_block_hash;
    }

    vm.vm.bootloader_state.push_l2_block(second_l2_block);

    vm.vm.push_transaction(l1_tx);

    let result = vm.vm.execute(VmExecutionMode::OneTx);
    if let Some(err) = expected_error {
        assert_eq!(result.result, ExecutionResult::Halt { reason: err });
    } else {
        assert_eq!(result.result, ExecutionResult::Success { output: vec![] });
    }
}

#[test]
fn test_l2_block_new_l2_block() {
    // This test is aimed to cover potential issue

    let correct_first_block = L2BlockEnv {
        number: 1,
        timestamp: 1,
        prev_block_hash: legacy_miniblock_hash(MiniblockNumber(0)),
        max_virtual_blocks_to_create: 1,
    };

    // Case 1: Block number increasing by more than 1
    test_new_l2_block(
        correct_first_block,
        Some(3),
        None,
        None,
        Some(Halt::FailedToSetL2Block(
            "Invalid new L2 block number".to_string(),
        )),
    );

    // Case 2: Timestamp not increasing
    test_new_l2_block(
        correct_first_block,
        None,
        Some(1),
        None,
        Some(Halt::FailedToSetL2Block("The timestamp of the new L2 block must be greater than the timestamp of the previous L2 block".to_string())),
    );

    // Case 3: Incorrect previous block hash
    test_new_l2_block(
        correct_first_block,
        None,
        None,
        Some(H256::zero()),
        Some(Halt::FailedToSetL2Block(
            "The current L2 block hash is incorrect".to_string(),
        )),
    );

    // Case 4: Correct new block
    test_new_l2_block(correct_first_block, None, None, None, None);
}

#[allow(clippy::too_many_arguments)]
fn test_first_in_batch(
    miniblock_timestamp: u64,
    miniblock_number: u32,
    pending_txs_hash: H256,
    batch_timestamp: u64,
    new_batch_timestamp: u64,
    batch_number: u32,
    proposed_block: L2BlockEnv,
    expected_error: Option<Halt>,
) {
    let mut l1_batch = default_l1_batch(L1BatchNumber(1));
    l1_batch.number += 1;
    l1_batch.timestamp = new_batch_timestamp;

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_l1_batch_env(l1_batch)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();
    let l1_tx = get_l1_noop();

    // Setting the values provided.
    let storage_ptr = vm.vm.state.storage.storage.get_ptr();
    let miniblock_info_slot = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    );
    let pending_txs_hash_slot = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
    );
    let batch_info_slot = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_BLOCK_INFO_POSITION,
    );
    let prev_block_hash_position = get_l2_block_hash_key(miniblock_number - 1);

    storage_ptr.borrow_mut().set_value(
        miniblock_info_slot,
        u256_to_h256(pack_block_info(
            miniblock_number as u64,
            miniblock_timestamp,
        )),
    );
    storage_ptr
        .borrow_mut()
        .set_value(pending_txs_hash_slot, pending_txs_hash);
    storage_ptr.borrow_mut().set_value(
        batch_info_slot,
        u256_to_h256(pack_block_info(batch_number as u64, batch_timestamp)),
    );
    storage_ptr.borrow_mut().set_value(
        prev_block_hash_position,
        legacy_miniblock_hash(MiniblockNumber(miniblock_number - 1)),
    );

    // In order to skip checks from the Rust side of the VM, we firstly use some definitely correct L2 block info.
    // And then override it with the user-provided value

    let last_l2_block = vm.vm.bootloader_state.last_l2_block();
    let new_l2_block = L2BlockEnv {
        number: last_l2_block.number + 1,
        timestamp: last_l2_block.timestamp + 1,
        prev_block_hash: last_l2_block.get_hash(),
        max_virtual_blocks_to_create: last_l2_block.max_virtual_blocks_to_create,
    };

    vm.vm.bootloader_state.push_l2_block(new_l2_block);
    vm.vm.push_transaction(l1_tx);
    let timestamp = Timestamp(vm.vm.state.local_state.timestamp);
    set_manual_l2_block_info(&mut vm.vm, 0, proposed_block, timestamp);

    let result = vm.vm.execute(VmExecutionMode::OneTx);
    if let Some(err) = expected_error {
        assert_eq!(result.result, ExecutionResult::Halt { reason: err });
    } else {
        assert_eq!(result.result, ExecutionResult::Success { output: vec![] });
    }
}

#[test]
fn test_l2_block_first_in_batch() {
    test_first_in_batch(
        1,
        1,
        H256::zero(),
        1,
        2,
        1,
        L2BlockEnv {
            number: 2,
            timestamp: 2,
            prev_block_hash: miniblock_hash(
                MiniblockNumber(1),
                1,
                legacy_miniblock_hash(MiniblockNumber(0)),
                H256::zero(),
            ),
            max_virtual_blocks_to_create: 1,
        },
        None,
    );

    test_first_in_batch(
        8,
        1,
        H256::zero(),
        5,
        12,
        1,
        L2BlockEnv {
            number: 2,
            timestamp: 9,
            prev_block_hash: miniblock_hash(MiniblockNumber(1), 8, legacy_miniblock_hash(MiniblockNumber(0)), H256::zero()),
            max_virtual_blocks_to_create: 1
        },
        Some(Halt::FailedToSetL2Block("The timestamp of the L2 block must be greater than or equal to the timestamp of the current batch".to_string())),
    );
}

fn set_manual_l2_block_info<S: WriteStorage, H: HistoryMode>(
    vm: &mut Vm<S, H>,
    tx_number: usize,
    block_info: L2BlockEnv,
    timestamp: Timestamp,
) {
    let fictive_miniblock_position =
        TX_OPERATOR_L2_BLOCK_INFO_OFFSET + TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO * tx_number;

    vm.state.memory.populate_page(
        BOOTLOADER_HEAP_PAGE as usize,
        vec![
            (fictive_miniblock_position, block_info.number.into()),
            (fictive_miniblock_position + 1, block_info.timestamp.into()),
            (
                fictive_miniblock_position + 2,
                h256_to_u256(block_info.prev_block_hash),
            ),
            (
                fictive_miniblock_position + 3,
                block_info.max_virtual_blocks_to_create.into(),
            ),
        ],
        timestamp,
    )
}
