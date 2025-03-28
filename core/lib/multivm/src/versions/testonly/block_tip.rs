use ethabi::Token;
use itertools::Itertools;
use zksync_contracts::load_sys_contract;
use zksync_system_constants::{
    CONTRACT_FORCE_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L1_MESSENGER_ADDRESS,
};
use zksync_test_contracts::TestContract;
use zksync_types::{
    bytecode::BytecodeHash, commitment::SerializeCommitment, fee_model::BatchFeeInput,
    get_code_key, l2_to_l1_log::L2ToL1Log, u256_to_h256, writes::StateDiffRecord, Address, Execute,
    H256, U256,
};

use super::{
    default_pubdata_builder, get_empty_storage,
    tester::{TestedVm, VmTesterBuilder},
};
use crate::{
    interface::{InspectExecutionMode, L1BatchEnv, TxExecutionMode, VmInterfaceExt},
    versions::testonly::default_l1_batch,
    vm_latest::constants::{
        BOOTLOADER_BATCH_TIP_CIRCUIT_STATISTICS_OVERHEAD,
        BOOTLOADER_BATCH_TIP_METRICS_SIZE_OVERHEAD, BOOTLOADER_BATCH_TIP_OVERHEAD,
        MAX_VM_PUBDATA_PER_BATCH,
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

const CALLS_PER_TX: usize = 1_000;

fn populate_mimic_calls(data: L1MessengerTestData) -> Vec<Vec<u8>> {
    let complex_upgrade = TestContract::complex_upgrade();
    let l1_messenger = load_sys_contract("L1Messenger");

    let logs_mimic_calls = (0..data.l2_to_l1_logs).map(|i| MimicCallInfo {
        to: L1_MESSENGER_ADDRESS,
        who_to_mimic: KNOWN_CODES_STORAGE_ADDRESS,
        data: l1_messenger
            .function("sendL2ToL1Log")
            .unwrap()
            .encode_input(&[
                Token::Bool(false),
                Token::FixedBytes(H256::from_low_u64_be(2 * i as u64).0.to_vec()),
                Token::FixedBytes(H256::from_low_u64_be(2 * i as u64 + 1).0.to_vec()),
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
            .encode_input(&[Token::FixedBytes(
                BytecodeHash::for_bytecode(bytecode).value().0.to_vec(),
            )])
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
        .chunks(CALLS_PER_TX)
        .into_iter()
        .map(|chunk| {
            complex_upgrade
                .function("mimicCalls")
                .encode_input(&[Token::Array(chunk.collect_vec())])
                .unwrap()
        })
        .collect_vec();

    encoded_calls
}

struct TestStatistics {
    pub max_used_gas: u32,
    pub circuit_statistics: u64,
    pub execution_metrics_size: u64,
}

struct StatisticsTagged {
    pub statistics: TestStatistics,
    pub tag: String,
}

fn execute_test<VM: TestedVm>(test_data: L1MessengerTestData) -> TestStatistics {
    let mut storage = get_empty_storage();
    let complex_upgrade_code = TestContract::complex_upgrade().bytecode.to_vec();

    // For this test we'll just put the bytecode onto the force deployer address
    storage.set_value(
        get_code_key(&CONTRACT_FORCE_DEPLOYER_ADDRESS),
        BytecodeHash::for_bytecode(&complex_upgrade_code).value(),
    );
    storage.store_factory_dep(
        BytecodeHash::for_bytecode(&complex_upgrade_code).value(),
        complex_upgrade_code,
    );

    // We are measuring computational cost, so prices for pubdata don't matter, while they artificially dilute
    // the gas limit

    let batch_env = L1BatchEnv {
        fee_input: BatchFeeInput::pubdata_independent(100_000, 100_000, 100_000),
        ..default_l1_batch(zksync_types::L1BatchNumber(1))
    };

    let mut vm = VmTesterBuilder::new()
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_l1_batch_env(batch_env)
        .build::<VM>();

    let bytecodes: Vec<_> = test_data.bytecodes.iter().map(Vec::as_slice).collect();
    vm.vm.insert_bytecodes(&bytecodes);

    let txs_data = populate_mimic_calls(test_data.clone());
    let account = &mut vm.rich_accounts[0];

    for (i, data) in txs_data.into_iter().enumerate() {
        let tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(CONTRACT_FORCE_DEPLOYER_ADDRESS),
                calldata: data,
                value: U256::zero(),
                factory_deps: vec![],
            },
            None,
        );

        vm.vm.push_transaction(tx);

        let result = vm.vm.execute(InspectExecutionMode::OneTx);
        assert!(
            !result.result.is_failed(),
            "Transaction {i} wasn't successful for input: {:#?}",
            test_data
        );
    }

    // Now we count how much ergs were spent at the end of the batch
    // It is assumed that the top level frame is the bootloader
    let gas_before = vm.vm.gas_remaining();
    let result = vm
        .vm
        .finish_batch_with_state_diffs(test_data.state_diffs.clone(), default_pubdata_builder());
    assert!(
        !result.result.is_failed(),
        "Batch wasn't successful for input: {test_data:?}"
    );
    let gas_after = vm.vm.gas_remaining();
    assert_eq!((gas_before - gas_after) as u64, result.statistics.gas_used);

    TestStatistics {
        max_used_gas: gas_before - gas_after,
        circuit_statistics: result.statistics.circuit_statistic.total() as u64,
        execution_metrics_size: result.get_execution_metrics().size() as u64,
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

pub(crate) fn test_dry_run_upper_bound<VM: TestedVm>() {
    // Some of the pubdata is consumed by constant fields (such as length of messages, number of logs, etc.).
    // While this leaves some room for error, at the end of the test we require that the `BOOTLOADER_BATCH_TIP_OVERHEAD`
    // is sufficient with a very large margin, so it is okay to ignore 1% of possible pubdata.
    const MAX_EFFECTIVE_PUBDATA_PER_BATCH: usize =
        (MAX_VM_PUBDATA_PER_BATCH as f64 * 0.99) as usize;

    // We are re-using the `ComplexUpgrade` contract as it already has the `mimicCall` functionality.
    // To get the upper bound, we'll try to do the following:
    // 1. Max number of logs.
    // 2. Lots of small L2->L1 messages / one large L2->L1 message.
    // 3. Lots of small bytecodes / one large bytecode.
    // 4. Lots of storage slot updates.

    let statistics = vec![
        // max logs
        StatisticsTagged {
            statistics: execute_test::<VM>(L1MessengerTestData {
                l2_to_l1_logs: MAX_EFFECTIVE_PUBDATA_PER_BATCH / L2ToL1Log::SERIALIZED_SIZE,
                ..Default::default()
            }),
            tag: "max_logs".to_string(),
        },
        // max messages
        StatisticsTagged {
            statistics: execute_test::<VM>(L1MessengerTestData {
                // Each L2->L1 message is accompanied by a Log + its length, which is a 4 byte number,
                // so the max number of pubdata is bound by it
                messages: vec![
                    vec![0; 0];
                    MAX_EFFECTIVE_PUBDATA_PER_BATCH / (L2ToL1Log::SERIALIZED_SIZE + 4)
                ],
                ..Default::default()
            }),
            tag: "max_messages".to_string(),
        },
        // long message
        StatisticsTagged {
            statistics: execute_test::<VM>(L1MessengerTestData {
                // Each L2->L1 message is accompanied by a Log, so the max number of pubdata is bound by it
                messages: vec![vec![0; MAX_EFFECTIVE_PUBDATA_PER_BATCH]; 1],
                ..Default::default()
            }),
            tag: "long_message".to_string(),
        },
        // max bytecodes
        StatisticsTagged {
            statistics: execute_test::<VM>(L1MessengerTestData {
                // Each bytecode must be at least 32 bytes long.
                // Each uncompressed bytecode is accompanied by its length, which is a 4 byte number
                bytecodes: vec![vec![0; 32]; MAX_EFFECTIVE_PUBDATA_PER_BATCH / (32 + 4)],
                ..Default::default()
            }),
            tag: "max_bytecodes".to_string(),
        },
        // long bytecode
        StatisticsTagged {
            statistics: execute_test::<VM>(L1MessengerTestData {
                bytecodes: vec![
                    vec![0; get_valid_bytecode_length(MAX_EFFECTIVE_PUBDATA_PER_BATCH)];
                    1
                ],
                ..Default::default()
            }),
            tag: "long_bytecode".to_string(),
        },
        // lots of small repeated writes
        StatisticsTagged {
            statistics: execute_test::<VM>(L1MessengerTestData {
                // In theory each state diff can require only 5 bytes to be published (enum index + 4 bytes for the key)
                state_diffs: generate_state_diffs(true, true, MAX_EFFECTIVE_PUBDATA_PER_BATCH / 5),
                ..Default::default()
            }),
            tag: "small_repeated_writes".to_string(),
        },
        // lots of big repeated writes
        StatisticsTagged {
            statistics: execute_test::<VM>(L1MessengerTestData {
                // Each big repeated write will approximately require 4 bytes for key + 1 byte for encoding type + 32 bytes for value
                state_diffs: generate_state_diffs(
                    true,
                    false,
                    MAX_EFFECTIVE_PUBDATA_PER_BATCH / 37,
                ),
                ..Default::default()
            }),
            tag: "big_repeated_writes".to_string(),
        },
        // lots of small initial writes
        StatisticsTagged {
            statistics: execute_test::<VM>(L1MessengerTestData {
                // Each small initial write will take at least 32 bytes for derived key + 1 bytes encoding zeroing out
                state_diffs: generate_state_diffs(
                    false,
                    true,
                    MAX_EFFECTIVE_PUBDATA_PER_BATCH / 33,
                ),
                ..Default::default()
            }),
            tag: "small_initial_writes".to_string(),
        },
        // lots of large initial writes
        StatisticsTagged {
            statistics: execute_test::<VM>(L1MessengerTestData {
                // Each big write will take at least 32 bytes for derived key + 1 byte for encoding type + 32 bytes for value
                state_diffs: generate_state_diffs(
                    false,
                    false,
                    MAX_EFFECTIVE_PUBDATA_PER_BATCH / 65,
                ),
                ..Default::default()
            }),
            tag: "big_initial_writes".to_string(),
        },
    ];

    // We use 2x overhead for the batch tip compared to the worst estimated scenario.
    let max_used_gas = statistics
        .iter()
        .map(|s| (s.statistics.max_used_gas, s.tag.clone()))
        .max()
        .unwrap();
    assert!(
        max_used_gas.0 * 3 / 2 <= BOOTLOADER_BATCH_TIP_OVERHEAD,
        "BOOTLOADER_BATCH_TIP_OVERHEAD is too low for {} with result {}, BOOTLOADER_BATCH_TIP_OVERHEAD = {}",
        max_used_gas.1,
        max_used_gas.0,
        BOOTLOADER_BATCH_TIP_OVERHEAD
    );

    let circuit_statistics = statistics
        .iter()
        .map(|s| (s.statistics.circuit_statistics, s.tag.clone()))
        .max()
        .unwrap();
    assert!(
        circuit_statistics.0 * 3 / 2 <= BOOTLOADER_BATCH_TIP_CIRCUIT_STATISTICS_OVERHEAD as u64,
        "BOOTLOADER_BATCH_TIP_CIRCUIT_STATISTICS_OVERHEAD is too low for {} with result {}, BOOTLOADER_BATCH_TIP_CIRCUIT_STATISTICS_OVERHEAD = {}",
        circuit_statistics.1,
        circuit_statistics.0,
        BOOTLOADER_BATCH_TIP_CIRCUIT_STATISTICS_OVERHEAD
    );

    let execution_metrics_size = statistics
        .iter()
        .map(|s| (s.statistics.execution_metrics_size, s.tag.clone()))
        .max()
        .unwrap();
    assert!(
        execution_metrics_size.0 * 3 / 2 <= BOOTLOADER_BATCH_TIP_METRICS_SIZE_OVERHEAD as u64,
        "BOOTLOADER_BATCH_TIP_METRICS_SIZE_OVERHEAD is too low for {} with result {}, BOOTLOADER_BATCH_TIP_METRICS_SIZE_OVERHEAD = {}",
        execution_metrics_size.1,
        execution_metrics_size.0,
        BOOTLOADER_BATCH_TIP_METRICS_SIZE_OVERHEAD
    );
}
