use std::borrow::BorrowMut;

use ethabi::Token;
use zk_evm_1_4_1::aux_structures::Timestamp;
use zksync_contracts::load_sys_contract;
use zksync_system_constants::{
    CONTRACT_FORCE_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L1_MESSENGER_ADDRESS,
};
use zksync_types::{
    commitment::SerializeCommitment, get_code_key, l2_to_l1_log::L2ToL1Log,
    writes::StateDiffRecord, Address, Execute, H256, U256,
};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256};

use super::utils::{get_complex_upgrade_abi, read_complex_upgrade};
use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        constants::{
            BOOTLOADER_BATCH_TIP_OVERHEAD_CIRCUITS, BOOTLOADER_BATCH_TIP_OVERHEAD_GAS,
            MAX_PUBDATA_PER_BATCH,
        },
        tests::tester::{get_empty_storage, InMemoryStorageView, VmTesterBuilder},
        tracers::PubdataTracer,
        HistoryEnabled, TracerDispatcher,
    },
};

#[derive(Debug, Clone, Default)]
struct L1MessengerTestData {
    l2_to_l1_logs: usize,
    messages: Vec<Vec<u8>>,
    bytecodes: Vec<Vec<u8>>,
    state_diffs: Vec<StateDiffRecord>,
}

struct MimicCallInfo {
    to: Address,
    who_to_mimic: Address,
    data: Vec<u8>,
}

fn populate_mimic_calls(data: L1MessengerTestData) -> Vec<u8> {
    let complex_upgrade = get_complex_upgrade_abi();
    let l1_messenger = load_sys_contract("L1Messenger");

    let logs_mimic_calls = (0..data.l2_to_l1_logs).map(|_| MimicCallInfo {
        to: L1_MESSENGER_ADDRESS,
        who_to_mimic: KNOWN_CODES_STORAGE_ADDRESS,
        data: l1_messenger
            .function("sendL2ToL1Log")
            .unwrap()
            .encode_input(&[
                Token::Bool(false),
                Token::FixedBytes(H256::random().0.to_vec()),
                Token::FixedBytes(H256::random().0.to_vec()),
            ])
            .unwrap(),
    });
    let messages_mimic_calls = data.messages.iter().map(|message| MimicCallInfo {
        to: L1_MESSENGER_ADDRESS,
        who_to_mimic: KNOWN_CODES_STORAGE_ADDRESS,
        data: l1_messenger
            .function("sendToL1")
            .unwrap()
            .encode_input(&[Token::Bytes(message.clone())])
            .unwrap(),
    });
    let bytecodes_mimic_calls = data.bytecodes.iter().map(|bytecode| MimicCallInfo {
        to: L1_MESSENGER_ADDRESS,
        who_to_mimic: KNOWN_CODES_STORAGE_ADDRESS,
        data: l1_messenger
            .function("requestBytecodeL1Publication")
            .unwrap()
            .encode_input(&[Token::FixedBytes(hash_bytecode(bytecode).0.to_vec())])
            .unwrap(),
    });

    let encoded_calls = logs_mimic_calls
        .chain(messages_mimic_calls)
        .chain(bytecodes_mimic_calls)
        .map(|call| {
            Token::Tuple(vec![
                Token::Address(call.to),
                Token::Address(call.who_to_mimic),
                Token::Bytes(call.data),
            ])
        })
        .collect::<Vec<_>>();

    complex_upgrade
        .function("mimicCalls")
        .unwrap()
        .encode_input(&[Token::Array(encoded_calls)])
        .unwrap()
}

#[derive(Debug, Clone, Default, Copy)]
struct OverheadTestResult {
    required_gas: u32,
    required_circuits: u32,
}

impl OverheadTestResult {
    fn stricter(&self, other: &Self) -> Self {
        Self {
            required_gas: self.required_gas.max(other.required_gas),
            required_circuits: self.required_circuits.max(other.required_circuits),
        }
    }
}

fn execute_test(test_data: L1MessengerTestData) -> OverheadTestResult {
    let mut storage = get_empty_storage();
    let complex_upgrade_code = read_complex_upgrade();

    // For this test we'll just put the bytecode onto the force deployer address
    storage.borrow_mut().set_value(
        get_code_key(&CONTRACT_FORCE_DEPLOYER_ADDRESS),
        hash_bytecode(&complex_upgrade_code),
    );
    storage
        .borrow_mut()
        .store_factory_dep(hash_bytecode(&complex_upgrade_code), complex_upgrade_code);

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let bytecodes = test_data
        .bytecodes
        .iter()
        .map(|bytecode| {
            let hash = hash_bytecode(bytecode);
            let words = bytes_to_be_words(bytecode.clone());
            (h256_to_u256(hash), words)
        })
        .collect();
    vm.vm
        .state
        .decommittment_processor
        .populate(bytecodes, Timestamp(0));

    let data = populate_mimic_calls(test_data.clone());
    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: CONTRACT_FORCE_DEPLOYER_ADDRESS,
            calldata: data,
            value: U256::zero(),
            factory_deps: None,
        },
        None,
    );

    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");

    // Now we count how much ergs were spent at the end of the batch
    // It is assumed that the top level frame is the bootloader

    let ergs_before = vm.vm.state.local_state.callstack.current.ergs_remaining;

    // We ensure that indeed the provided state diffs are used
    let pubdata_tracer = PubdataTracer::<InMemoryStorageView>::new_with_forced_state_diffs(
        vm.vm.batch_env.clone(),
        VmExecutionMode::Batch,
        test_data.state_diffs,
    );

    let result = vm.vm.inspect_inner(
        TracerDispatcher::default(),
        VmExecutionMode::Batch,
        Some(pubdata_tracer),
    );

    assert!(!result.result.is_failed(), "Batch wasn't successful");

    let ergs_after = vm.vm.state.local_state.callstack.current.ergs_remaining;

    let required_gas = ergs_before - ergs_after;
    let required_circuits = result.statistics.circuit_statistic.total() as u32;

    OverheadTestResult {
        required_gas,
        required_circuits,
    }
}

fn generate_state_diffs(
    repeated_writes: bool,
    small_diff: bool,
    number_of_state_diffs: usize,
) -> Vec<StateDiffRecord> {
    (0..number_of_state_diffs)
        .map(|i| {
            let address = Address::from_low_u64_be(i as u64);
            let key = U256::from(i);
            let enumeration_index = if repeated_writes { i + 1 } else { 0 };

            let (initial_value, final_value) = if small_diff {
                // As small as it gets, one byte to denote zeroing out the value
                (U256::from(1), U256::from(0))
            } else {
                // As large as it gets
                (U256::from(0), U256::from(2).pow(255.into()))
            };

            StateDiffRecord {
                address,
                key,
                derived_key: u256_to_h256(i.into()).0,
                enumeration_index: enumeration_index as u64,
                initial_value,
                final_value,
            }
        })
        .collect()
}

// A valid zkEVM bytecode has odd number of 32 byte words
fn get_valid_bytecode_length(length: usize) -> usize {
    // Firstly ensure that the length is divisible by 32
    let length_padded_to_32 = if length % 32 == 0 {
        length
    } else {
        length + 32 - (length % 32)
    };

    // Then we ensure that the number returned by division by 32 is odd
    if length_padded_to_32 % 64 == 0 {
        length_padded_to_32 + 32
    } else {
        length_padded_to_32
    }
}

#[test]
fn test_dry_run_upper_bound() {
    // We are re-using the `ComplexUpgrade` contract as it already has the `mimicCall` functionality.
    // To get the upper bound, we'll try to do the following:
    // 1. Max number of logs.
    // 2. Lots of small L2->L1 messages / one large L2->L1 message.
    // 3. Lots of small bytecodes / one large bytecode.
    // 4. Lots of storage slot updates.

    // Some of the pubdata is consumed by constant fields (such as length of messages, number of logs, etc.).
    // While this leaves some room for error, at the end of the test we require that the `BOOTLOADER_BATCH_TIP_OVERHEAD_GAS`
    // is sufficient with a very large margin, so it is okay to ignore 1% of possible pubdata.
    const MAX_EFFECTIVE_PUBDATA_PER_BATCH: usize = (MAX_PUBDATA_PER_BATCH as f64 * 0.99) as usize;

    let max_logs = execute_test(L1MessengerTestData {
        l2_to_l1_logs: MAX_EFFECTIVE_PUBDATA_PER_BATCH as usize / L2ToL1Log::SERIALIZED_SIZE,
        ..Default::default()
    });

    let max_messages = execute_test(L1MessengerTestData {
        // Each L2->L1 message is accompanied by a Log + its length, which is a 4 byte number
        messages: vec![
            vec![0; 0];
            MAX_EFFECTIVE_PUBDATA_PER_BATCH as usize / (L2ToL1Log::SERIALIZED_SIZE + 4)
        ],
        ..Default::default()
    });

    let long_message = execute_test(L1MessengerTestData {
        messages: vec![vec![0; MAX_EFFECTIVE_PUBDATA_PER_BATCH]; 1],
        ..Default::default()
    });

    let max_bytecodes = execute_test(L1MessengerTestData {
        // Each uncompressed bytecode is accompanied by its length, which is a 4 byte number
        bytecodes: vec![vec![0; 32]; MAX_EFFECTIVE_PUBDATA_PER_BATCH as usize / (32 + 4)],
        ..Default::default()
    });

    let long_bytecode = execute_test(L1MessengerTestData {
        // We have to add 48 since a valid bytecode must have an odd number of 32 byte words
        bytecodes: vec![vec![0; get_valid_bytecode_length(MAX_EFFECTIVE_PUBDATA_PER_BATCH)]; 1],
        ..Default::default()
    });

    let lots_of_small_repeated_writes = execute_test(L1MessengerTestData {
        // In theory each state diff can require only 5 bytes to be published (enum index + 4 bytes for the key)
        state_diffs: generate_state_diffs(true, true, MAX_EFFECTIVE_PUBDATA_PER_BATCH as usize / 5),
        ..Default::default()
    });

    let lots_of_big_repeated_writes = execute_test(L1MessengerTestData {
        // Each big repeated write will approximately require 4 bytes for key + 1 byte for encoding type + 32 bytes for value
        state_diffs: generate_state_diffs(
            true,
            false,
            MAX_EFFECTIVE_PUBDATA_PER_BATCH as usize / 37,
        ),
        ..Default::default()
    });

    let lots_of_small_initial_writes = execute_test(L1MessengerTestData {
        // Each small initial write will take at least 32 bytes for derived key + 1 bytes encoding zeroing out
        state_diffs: generate_state_diffs(
            false,
            true,
            MAX_EFFECTIVE_PUBDATA_PER_BATCH as usize / 33,
        ),
        ..Default::default()
    });

    let lots_of_large_initial_writes = execute_test(L1MessengerTestData {
        // Each big write will take at least 32 bytes for derived key + 1 byte for encoding type + 32 bytes for value
        state_diffs: generate_state_diffs(
            false,
            false,
            MAX_EFFECTIVE_PUBDATA_PER_BATCH as usize / 65,
        ),
        ..Default::default()
    });

    let max_result = vec![
        max_logs,
        max_messages,
        long_message,
        max_bytecodes,
        long_bytecode,
        lots_of_small_repeated_writes,
        lots_of_big_repeated_writes,
        lots_of_small_initial_writes,
        lots_of_large_initial_writes,
    ]
    .into_iter()
    .fold(OverheadTestResult::default(), |acc, x| acc.stricter(&x));

    // We use 2x overhead for the batch tip compared to the worst estimated scenario.
    assert!(
        max_result.required_gas * 2 <= BOOTLOADER_BATCH_TIP_OVERHEAD_GAS,
        "BOOTLOADER_BATCH_TIP_OVERHEAD_GAS is too low"
    );

    assert!(
        max_result.required_circuits * 2 <= BOOTLOADER_BATCH_TIP_OVERHEAD_CIRCUITS,
        "BOOTLOADER_BATCH_TIP_OVERHEAD_CIRCUITS is too low"
    );
}
