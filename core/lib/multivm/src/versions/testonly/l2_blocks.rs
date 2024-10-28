//!
//! Tests for the bootloader
//! The description for each of the tests can be found in the corresponding `.yul` file.
//!

use assert_matches::assert_matches;
use zksync_system_constants::REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    block::{pack_block_info, L2BlockHasher},
    AccountTreeId, Address, Execute, ExecuteTransactionCommon, L1BatchNumber, L1TxCommonData,
    L2BlockNumber, ProtocolVersionId, StorageKey, Transaction, H256, SYSTEM_CONTEXT_ADDRESS,
    SYSTEM_CONTEXT_BLOCK_INFO_POSITION, SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION, U256,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

use super::{default_l1_batch, get_empty_storage, tester::VmTesterBuilder, TestedVm};
use crate::{
    interface::{
        storage::StorageView, ExecutionResult, Halt, InspectExecutionMode, L2BlockEnv,
        TxExecutionMode, VmInterfaceExt,
    },
    vm_latest::{
        constants::{TX_OPERATOR_L2_BLOCK_INFO_OFFSET, TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO},
        utils::l2_blocks::get_l2_block_hash_key,
    },
};

fn get_l1_noop() -> Transaction {
    Transaction {
        common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
            sender: Address::repeat_byte(1),
            gas_limit: U256::from(2000000u32),
            gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
            ..Default::default()
        }),
        execute: Execute {
            contract_address: Some(Address::repeat_byte(0xc0)),
            calldata: vec![],
            value: U256::zero(),
            factory_deps: vec![],
        },
        received_timestamp_ms: 0,
        raw_bytes: None,
    }
}

pub(crate) fn test_l2_block_initialization_timestamp<VM: TestedVm>() {
    // This test checks that the L2 block initialization works correctly.
    // Here we check that the first block must have timestamp that is greater or equal to the timestamp
    // of the current batch.

    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    // Override the timestamp of the current L2 block to be 0.
    vm.vm.push_l2_block_unchecked(L2BlockEnv {
        number: 1,
        timestamp: 0,
        prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
        max_virtual_blocks_to_create: 1,
    });
    let l1_tx = get_l1_noop();

    vm.vm.push_transaction(l1_tx);
    let res = vm.vm.execute(InspectExecutionMode::OneTx);

    assert_matches!(
        res.result,
        ExecutionResult::Halt { reason: Halt::FailedToSetL2Block(msg) }
            if msg.contains("timestamp")
    );
}

pub(crate) fn test_l2_block_initialization_number_non_zero<VM: TestedVm>() {
    // This test checks that the L2 block initialization works correctly.
    // Here we check that the first L2 block number can not be zero.

    let l1_batch = default_l1_batch(L1BatchNumber(1));
    let first_l2_block = L2BlockEnv {
        number: 0,
        timestamp: l1_batch.timestamp,
        prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
        max_virtual_blocks_to_create: 1,
    };

    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_l1_batch_env(l1_batch)
        .with_rich_accounts(1)
        .build::<VM>();

    let l1_tx = get_l1_noop();

    vm.vm.push_transaction(l1_tx);

    set_manual_l2_block_info(&mut vm.vm, 0, first_l2_block);

    let res = vm.vm.execute(InspectExecutionMode::OneTx);

    assert_eq!(
        res.result,
        ExecutionResult::Halt {
            reason: Halt::FailedToSetL2Block(
                "L2 block number is never expected to be zero".to_string()
            )
        }
    );
}

fn test_same_l2_block<VM: TestedVm>(
    expected_error: Option<Halt>,
    override_timestamp: Option<u64>,
    override_prev_block_hash: Option<H256>,
) {
    let mut l1_batch = default_l1_batch(L1BatchNumber(1));
    l1_batch.timestamp = 1;
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_l1_batch_env(l1_batch)
        .with_rich_accounts(1)
        .build::<VM>();

    let l1_tx = get_l1_noop();
    vm.vm.push_transaction(l1_tx.clone());
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!res.result.is_failed());

    let mut current_l2_block = vm.l1_batch_env.first_l2_block;

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
    set_manual_l2_block_info(&mut vm.vm, 1, current_l2_block);

    let result = vm.vm.execute(InspectExecutionMode::OneTx);

    if let Some(err) = expected_error {
        assert_eq!(result.result, ExecutionResult::Halt { reason: err });
    } else {
        assert_eq!(result.result, ExecutionResult::Success { output: vec![] });
    }
}

pub(crate) fn test_l2_block_same_l2_block<VM: TestedVm>() {
    // This test aims to test the case when there are multiple transactions inside the same L2 block.

    // Case 1: Incorrect timestamp
    test_same_l2_block::<VM>(
        Some(Halt::FailedToSetL2Block(
            "The timestamp of the same L2 block must be same".to_string(),
        )),
        Some(0),
        None,
    );

    // Case 2: Incorrect previous block hash
    test_same_l2_block::<VM>(
        Some(Halt::FailedToSetL2Block(
            "The previous hash of the same L2 block must be same".to_string(),
        )),
        None,
        Some(H256::zero()),
    );

    // Case 3: Correct continuation of the same L2 block
    test_same_l2_block::<VM>(None, None, None);
}

fn test_new_l2_block<VM: TestedVm>(
    first_l2_block: L2BlockEnv,
    overriden_second_block_number: Option<u32>,
    overriden_second_block_timestamp: Option<u64>,
    overriden_second_block_prev_block_hash: Option<H256>,
    expected_error: Option<Halt>,
) {
    let mut l1_batch = default_l1_batch(L1BatchNumber(1));
    l1_batch.timestamp = 1;
    l1_batch.first_l2_block = first_l2_block;

    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_l1_batch_env(l1_batch)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let l1_tx = get_l1_noop();

    // Firstly we execute the first transaction
    vm.vm.push_transaction(l1_tx.clone());
    vm.vm.execute(InspectExecutionMode::OneTx);

    let mut second_l2_block = vm.l1_batch_env.first_l2_block;
    second_l2_block.number += 1;
    second_l2_block.timestamp += 1;
    second_l2_block.prev_block_hash = vm.vm.last_l2_block_hash();

    if let Some(block_number) = overriden_second_block_number {
        second_l2_block.number = block_number;
    }
    if let Some(timestamp) = overriden_second_block_timestamp {
        second_l2_block.timestamp = timestamp;
    }
    if let Some(prev_block_hash) = overriden_second_block_prev_block_hash {
        second_l2_block.prev_block_hash = prev_block_hash;
    }

    vm.vm.push_l2_block_unchecked(second_l2_block);
    vm.vm.push_transaction(l1_tx);

    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    if let Some(err) = expected_error {
        assert_eq!(result.result, ExecutionResult::Halt { reason: err });
    } else {
        assert_eq!(result.result, ExecutionResult::Success { output: vec![] });
    }
}

pub(crate) fn test_l2_block_new_l2_block<VM: TestedVm>() {
    // This test is aimed to cover potential issue

    let correct_first_block = L2BlockEnv {
        number: 1,
        timestamp: 1,
        prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
        max_virtual_blocks_to_create: 1,
    };

    // Case 1: Block number increasing by more than 1
    test_new_l2_block::<VM>(
        correct_first_block,
        Some(3),
        None,
        None,
        Some(Halt::FailedToSetL2Block(
            "Invalid new L2 block number".to_string(),
        )),
    );

    // Case 2: Timestamp not increasing
    test_new_l2_block::<VM>(
        correct_first_block,
        None,
        Some(1),
        None,
        Some(Halt::FailedToSetL2Block("The timestamp of the new L2 block must be greater than the timestamp of the previous L2 block".to_string())),
    );

    // Case 3: Incorrect previous block hash
    test_new_l2_block::<VM>(
        correct_first_block,
        None,
        None,
        Some(H256::zero()),
        Some(Halt::FailedToSetL2Block(
            "The current L2 block hash is incorrect".to_string(),
        )),
    );

    // Case 4: Correct new block
    test_new_l2_block::<VM>(correct_first_block, None, None, None, None);
}

#[allow(clippy::too_many_arguments)]
fn test_first_in_batch<VM: TestedVm>(
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

    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_l1_batch_env(l1_batch)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();
    let l1_tx = get_l1_noop();

    // Setting the values provided.
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

    let mut storage = get_empty_storage();
    storage.set_value(
        miniblock_info_slot,
        u256_to_h256(pack_block_info(
            miniblock_number as u64,
            miniblock_timestamp,
        )),
    );
    storage.set_value(pending_txs_hash_slot, pending_txs_hash);
    storage.set_value(
        batch_info_slot,
        u256_to_h256(pack_block_info(batch_number as u64, batch_timestamp)),
    );
    storage.set_value(
        prev_block_hash_position,
        L2BlockHasher::legacy_hash(L2BlockNumber(miniblock_number - 1)),
    );
    // Replace the storage entirely. It's not enough to write to the underlying storage (since read values are already cached
    // in the storage view).
    *vm.storage.borrow_mut() = StorageView::new(storage);

    // In order to skip checks from the Rust side of the VM, we firstly use some definitely correct L2 block info.
    // And then override it with the user-provided value

    let last_l2_block = vm.l1_batch_env.first_l2_block;
    let new_l2_block = L2BlockEnv {
        number: last_l2_block.number + 1,
        timestamp: last_l2_block.timestamp + 1,
        prev_block_hash: vm.vm.last_l2_block_hash(),
        max_virtual_blocks_to_create: last_l2_block.max_virtual_blocks_to_create,
    };

    vm.vm.push_l2_block_unchecked(new_l2_block);
    vm.vm.push_transaction(l1_tx);
    set_manual_l2_block_info(&mut vm.vm, 0, proposed_block);

    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    if let Some(err) = expected_error {
        assert_eq!(result.result, ExecutionResult::Halt { reason: err });
    } else {
        assert_eq!(result.result, ExecutionResult::Success { output: vec![] });
    }
}

pub(crate) fn test_l2_block_first_in_batch<VM: TestedVm>() {
    let prev_block_hash = L2BlockHasher::legacy_hash(L2BlockNumber(0));
    let prev_block_hash = L2BlockHasher::new(L2BlockNumber(1), 1, prev_block_hash)
        .finalize(ProtocolVersionId::latest());
    test_first_in_batch::<VM>(
        1,
        1,
        H256::zero(),
        1,
        2,
        1,
        L2BlockEnv {
            number: 2,
            timestamp: 2,
            prev_block_hash,
            max_virtual_blocks_to_create: 1,
        },
        None,
    );

    let prev_block_hash = L2BlockHasher::legacy_hash(L2BlockNumber(0));
    let prev_block_hash = L2BlockHasher::new(L2BlockNumber(1), 8, prev_block_hash)
        .finalize(ProtocolVersionId::latest());
    test_first_in_batch::<VM>(
        8,
        1,
        H256::zero(),
        5,
        12,
        1,
        L2BlockEnv {
            number: 2,
            timestamp: 9,
            prev_block_hash,
            max_virtual_blocks_to_create: 1,
        },
        Some(Halt::FailedToSetL2Block("The timestamp of the L2 block must be greater than or equal to the timestamp of the current batch".to_string())),
    );
}

fn set_manual_l2_block_info(vm: &mut impl TestedVm, tx_number: usize, block_info: L2BlockEnv) {
    let fictive_miniblock_position =
        TX_OPERATOR_L2_BLOCK_INFO_OFFSET + TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO * tx_number;
    vm.write_to_bootloader_heap(&[
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
    ])
}
