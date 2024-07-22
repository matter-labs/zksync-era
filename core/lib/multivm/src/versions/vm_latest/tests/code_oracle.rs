use ethabi::Token;
use zk_evm_1_5_0::{
    aux_structures::{MemoryPage, Timestamp},
    zkevm_opcode_defs::{ContractCodeSha256Format, VersionedHashLen32},
};
use zksync_types::{
    get_known_code_key, web3::keccak256, Address, Execute, StorageLogWithPreviousValue, H256, U256,
};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        tests::{
            tester::{get_empty_storage, VmTesterBuilder},
            utils::{load_precompiles_contract, read_precompiles_contract, read_test_contract},
        },
        HistoryEnabled,
    },
};

fn generate_large_bytecode() -> Vec<u8> {
    // This is the maximal possible size of a zkEVM bytecode
    vec![2u8; ((1 << 16) - 1) * 32]
}

#[test]
fn test_code_oracle() {
    let precompiles_contract_address = Address::random();
    let precompile_contract_bytecode = read_precompiles_contract();

    // Filling the zkevm bytecode
    let normal_zkevm_bytecode = read_test_contract();
    let normal_zkevm_bytecode_hash = hash_bytecode(&normal_zkevm_bytecode);
    let normal_zkevm_bytecode_keccak_hash = keccak256(&normal_zkevm_bytecode);
    let mut storage = get_empty_storage();
    storage.set_value(
        get_known_code_key(&normal_zkevm_bytecode_hash),
        u256_to_h256(U256::one()),
    );

    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .with_custom_contracts(vec![(
            precompile_contract_bytecode,
            precompiles_contract_address,
            false,
        )])
        .with_storage(storage)
        .build();

    let precompile_contract = load_precompiles_contract();
    let call_code_oracle_function = precompile_contract.function("callCodeOracle").unwrap();

    vm.vm.state.decommittment_processor.populate(
        vec![(
            h256_to_u256(normal_zkevm_bytecode_hash),
            bytes_to_be_words(normal_zkevm_bytecode),
        )],
        Timestamp(0),
    );

    let account = &mut vm.rich_accounts[0];

    // Firstly, let's ensure that the contract works.
    let tx1 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: precompiles_contract_address,
            calldata: call_code_oracle_function
                .encode_input(&[
                    Token::FixedBytes(normal_zkevm_bytecode_hash.0.to_vec()),
                    Token::FixedBytes(normal_zkevm_bytecode_keccak_hash.to_vec()),
                ])
                .unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(tx1);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");

    // Now, we ask for the same bytecode. We use to partially check whether the memory page with
    // the decommitted bytecode gets erased (it shouldn't).
    let tx2 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: precompiles_contract_address,
            calldata: call_code_oracle_function
                .encode_input(&[
                    Token::FixedBytes(normal_zkevm_bytecode_hash.0.to_vec()),
                    Token::FixedBytes(normal_zkevm_bytecode_keccak_hash.to_vec()),
                ])
                .unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx2);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");
}

fn find_code_oracle_cost_log(
    precompiles_contract_address: Address,
    logs: &[StorageLogWithPreviousValue],
) -> &StorageLogWithPreviousValue {
    let log = logs.iter().find(|log| {
        *log.log.key.address() == precompiles_contract_address && *log.log.key.key() == H256::zero()
    });
    log.expect("no code oracle cost log")
}

#[test]
fn test_code_oracle_big_bytecode() {
    let precompiles_contract_address = Address::random();
    let precompile_contract_bytecode = read_precompiles_contract();

    let big_zkevm_bytecode = generate_large_bytecode();
    let big_zkevm_bytecode_hash = hash_bytecode(&big_zkevm_bytecode);
    let big_zkevm_bytecode_keccak_hash = keccak256(&big_zkevm_bytecode);

    let mut storage = get_empty_storage();
    storage.set_value(
        get_known_code_key(&big_zkevm_bytecode_hash),
        u256_to_h256(U256::one()),
    );

    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .with_custom_contracts(vec![(
            precompile_contract_bytecode,
            precompiles_contract_address,
            false,
        )])
        .with_storage(storage)
        .build();

    let precompile_contract = load_precompiles_contract();
    let call_code_oracle_function = precompile_contract.function("callCodeOracle").unwrap();

    vm.vm.state.decommittment_processor.populate(
        vec![(
            h256_to_u256(big_zkevm_bytecode_hash),
            bytes_to_be_words(big_zkevm_bytecode),
        )],
        Timestamp(0),
    );

    let account = &mut vm.rich_accounts[0];

    // Firstly, let's ensure that the contract works.
    let tx1 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: precompiles_contract_address,
            calldata: call_code_oracle_function
                .encode_input(&[
                    Token::FixedBytes(big_zkevm_bytecode_hash.0.to_vec()),
                    Token::FixedBytes(big_zkevm_bytecode_keccak_hash.to_vec()),
                ])
                .unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(tx1);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");
}

#[test]
fn refunds_in_code_oracle() {
    let precompiles_contract_address = Address::random();
    let precompile_contract_bytecode = read_precompiles_contract();

    let normal_zkevm_bytecode = read_test_contract();
    let normal_zkevm_bytecode_hash = hash_bytecode(&normal_zkevm_bytecode);
    let normal_zkevm_bytecode_keccak_hash = keccak256(&normal_zkevm_bytecode);
    let normal_zkevm_bytecode_words = bytes_to_be_words(normal_zkevm_bytecode);
    let mut storage = get_empty_storage();
    storage.set_value(
        get_known_code_key(&normal_zkevm_bytecode_hash),
        u256_to_h256(U256::one()),
    );

    let precompile_contract = load_precompiles_contract();
    let call_code_oracle_function = precompile_contract.function("callCodeOracle").unwrap();

    // Execute code oracle twice with identical VM state that only differs in that the queried bytecode
    // is already decommitted the second time. The second call must consume less gas (`decommit` doesn't charge additional gas
    // for already decommitted codes).
    let mut oracle_costs = vec![];
    for decommit in [false, true] {
        let mut vm = VmTesterBuilder::new(HistoryEnabled)
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_random_rich_accounts(1)
            .with_custom_contracts(vec![(
                precompile_contract_bytecode.clone(),
                precompiles_contract_address,
                false,
            )])
            .with_storage(storage.clone())
            .build();

        vm.vm.state.decommittment_processor.populate(
            vec![(
                h256_to_u256(normal_zkevm_bytecode_hash),
                normal_zkevm_bytecode_words.clone(),
            )],
            Timestamp(0),
        );

        let account = &mut vm.rich_accounts[0];
        if decommit {
            let (header, normalized_preimage) =
                ContractCodeSha256Format::normalize_for_decommitment(&normal_zkevm_bytecode_hash.0);
            let query = vm
                .vm
                .state
                .prepare_to_decommit(
                    0,
                    header,
                    normalized_preimage,
                    MemoryPage(123),
                    Timestamp(0),
                )
                .unwrap();

            assert!(query.is_fresh);
            vm.vm.state.execute_decommit(0, query).unwrap();
        }

        let tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: precompiles_contract_address,
                calldata: call_code_oracle_function
                    .encode_input(&[
                        Token::FixedBytes(normal_zkevm_bytecode_hash.0.to_vec()),
                        Token::FixedBytes(normal_zkevm_bytecode_keccak_hash.to_vec()),
                    ])
                    .unwrap(),
                value: U256::zero(),
                factory_deps: vec![],
            },
            None,
        );

        vm.vm.push_transaction(tx);
        let result = vm.vm.execute(VmExecutionMode::OneTx);
        assert!(!result.result.is_failed(), "Transaction wasn't successful");
        let log =
            find_code_oracle_cost_log(precompiles_contract_address, &result.logs.storage_logs);
        oracle_costs.push(log.log.value);
    }

    // The refund is equal to `gasCost` parameter passed to the `decommit` opcode, which is defined as `4 * contract_length_in_words`
    // in `CodeOracle.yul`.
    let code_oracle_refund = h256_to_u256(oracle_costs[0]) - h256_to_u256(oracle_costs[1]);
    assert_eq!(
        code_oracle_refund,
        (4 * normal_zkevm_bytecode_words.len()).into()
    );
}
