use ethabi::Token;
use zksync_test_contracts::TestContract;
use zksync_types::{
    bytecode::{BytecodeHash, MAX_BYTECODE_LENGTH_IN_WORDS},
    get_known_code_key, h256_to_u256, u256_to_h256,
    web3::keccak256,
    Address, Execute, StorageLogWithPreviousValue, H256, U256,
};

use super::{default_system_env, get_empty_storage, tester::VmTesterBuilder, TestedVm};
use crate::{
    interface::{
        storage::ReadStorage, ExecutionResult, InspectExecutionMode, TxExecutionMode,
        VmInterfaceExt,
    },
    versions::testonly::ContractToDeploy,
};

fn generate_large_bytecode() -> Vec<u8> {
    // This is the maximal possible size of a zkEVM bytecode
    vec![2u8; MAX_BYTECODE_LENGTH_IN_WORDS * 32]
}

pub(crate) fn test_code_oracle<VM: TestedVm>() {
    let precompiles_contract_address = Address::repeat_byte(1);
    let precompile_contract_bytecode = TestContract::precompiles_test().bytecode.to_vec();

    // Filling the zkevm bytecode
    let normal_zkevm_bytecode = TestContract::counter().bytecode;
    let normal_zkevm_bytecode_hash = BytecodeHash::for_bytecode(normal_zkevm_bytecode).value();
    let normal_zkevm_bytecode_keccak_hash = keccak256(normal_zkevm_bytecode);
    let mut storage = get_empty_storage();
    storage.set_value(
        get_known_code_key(&normal_zkevm_bytecode_hash),
        u256_to_h256(U256::one()),
    );

    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(
            precompile_contract_bytecode,
            precompiles_contract_address,
        )])
        .with_storage(storage)
        .build::<VM>();

    let precompile_contract = TestContract::precompiles_test();
    let call_code_oracle_function = precompile_contract.function("callCodeOracle");

    vm.vm.insert_bytecodes(&[normal_zkevm_bytecode]);
    let account = &mut vm.rich_accounts[0];

    // Firstly, let's ensure that the contract works.
    let tx1 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(precompiles_contract_address),
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
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "Transaction wasn't successful: {result:#?}"
    );

    // Now, we ask for the same bytecode. We use to partially check whether the memory page with
    // the decommitted bytecode gets erased (it shouldn't).
    let tx2 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(precompiles_contract_address),
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
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "Transaction wasn't successful: {result:#?}"
    );
}

fn find_code_oracle_cost_log(
    precompiles_contract_address: Address,
    logs: &[StorageLogWithPreviousValue],
) -> &StorageLogWithPreviousValue {
    logs.iter()
        .find(|log| {
            *log.log.key.address() == precompiles_contract_address && log.log.key.key().is_zero()
        })
        .expect("no code oracle cost log")
}

pub(crate) fn test_code_oracle_big_bytecode<VM: TestedVm>() {
    let precompiles_contract_address = Address::repeat_byte(1);
    let precompile_contract_bytecode = TestContract::precompiles_test().bytecode.to_vec();

    let big_zkevm_bytecode = generate_large_bytecode();
    let big_zkevm_bytecode_hash = BytecodeHash::for_bytecode(&big_zkevm_bytecode).value();
    let big_zkevm_bytecode_keccak_hash = keccak256(&big_zkevm_bytecode);

    let mut storage = get_empty_storage();
    storage.set_value(
        get_known_code_key(&big_zkevm_bytecode_hash),
        u256_to_h256(U256::one()),
    );

    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(
            precompile_contract_bytecode,
            precompiles_contract_address,
        )])
        .with_storage(storage)
        .build::<VM>();

    let precompile_contract = TestContract::precompiles_test();
    let call_code_oracle_function = precompile_contract.function("callCodeOracle");

    vm.vm.insert_bytecodes(&[big_zkevm_bytecode.as_slice()]);

    let account = &mut vm.rich_accounts[0];

    // Firstly, let's ensure that the contract works.
    let tx1 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(precompiles_contract_address),
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
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "Transaction wasn't successful: {result:#?}"
    );
}

pub(crate) fn test_refunds_in_code_oracle<VM: TestedVm>() {
    let precompiles_contract_address = Address::repeat_byte(1);

    let normal_zkevm_bytecode = TestContract::counter().bytecode;
    let normal_zkevm_bytecode_hash = BytecodeHash::for_bytecode(normal_zkevm_bytecode).value();
    let normal_zkevm_bytecode_keccak_hash = keccak256(normal_zkevm_bytecode);
    let mut storage = get_empty_storage();
    storage.set_value(
        get_known_code_key(&normal_zkevm_bytecode_hash),
        u256_to_h256(U256::one()),
    );

    let precompile_contract = TestContract::precompiles_test();
    let call_code_oracle_function = precompile_contract.function("callCodeOracle");

    // Execute code oracle twice with identical VM state that only differs in that the queried bytecode
    // is already decommitted the second time. The second call must consume less gas (`decommit` doesn't charge additional gas
    // for already decommitted codes).
    let mut oracle_costs = vec![];
    for decommit in [false, true] {
        let mut vm = VmTesterBuilder::new()
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_rich_accounts(1)
            .with_custom_contracts(vec![ContractToDeploy::new(
                TestContract::precompiles_test().bytecode.to_vec(),
                precompiles_contract_address,
            )])
            .with_storage(storage.clone())
            .build::<VM>();

        vm.vm.insert_bytecodes(&[normal_zkevm_bytecode]);

        let account = &mut vm.rich_accounts[0];
        if decommit {
            let is_fresh = vm.vm.manually_decommit(normal_zkevm_bytecode_hash);
            assert!(is_fresh);
        }

        let tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(precompiles_contract_address),
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
        let result = vm.vm.execute(InspectExecutionMode::OneTx);
        assert!(
            !result.result.is_failed(),
            "Transaction wasn't successful: {result:#?}"
        );
        let log =
            find_code_oracle_cost_log(precompiles_contract_address, &result.logs.storage_logs);
        oracle_costs.push(log.log.value);
    }

    // The refund is equal to `gasCost` parameter passed to the `decommit` opcode, which is defined as `4 * contract_length_in_words`
    // in `CodeOracle.yul`.
    let code_oracle_refund = h256_to_u256(oracle_costs[0]) - h256_to_u256(oracle_costs[1]);
    assert_eq!(
        code_oracle_refund,
        (4 * (normal_zkevm_bytecode.len() / 32)).into()
    );
}

/// One padded body has two valid versioned identifiers sharing the 28-byte normalized preimage: the
/// native EraVM hash (version 1) and the EVM hash (version 2). Decommitting either one must make the
/// other a *repeat*, so that `CodeOracle` refunds the `decommit` extra cost the second time.
/// See also `vm_latest::tests::decommit_alias`.
pub(crate) fn test_cross_version_alias_in_code_oracle<VM: TestedVm>() {
    let precompiles_contract_address = Address::repeat_byte(1);
    // Word aligned with an odd word count, so `pad_evm_bytecode` is the identity on it and both hash
    // constructors digest the very same bytes.
    let body = vec![0x5b_u8; 47 * 32];
    let native_hash = BytecodeHash::for_bytecode(&body).value();
    let evm_hash = BytecodeHash::for_evm_bytecode(body.len(), &body).value();
    assert_eq!(native_hash.0[4..], evm_hash.0[4..]);
    let body_keccak_hash = keccak256(&body);

    let mut storage = get_empty_storage();
    for hash in [native_hash, evm_hash] {
        storage.set_value(get_known_code_key(&hash), u256_to_h256(U256::one()));
        storage.store_factory_dep(hash, body.clone());
    }

    let precompile_contract = TestContract::precompiles_test();
    let call_code_oracle_function = precompile_contract.function("callCodeOracle");

    // Measures the very same `CodeOracle(queried_hash)` transaction in VM states that differ only in
    // which identifier was decommitted beforehand, so the whole difference in the reported cost is
    // the `decommit` extra cost.
    let measure_oracle_cost = |queried_hash: H256, prewarmed_hash: Option<H256>| -> U256 {
        let mut vm = VmTesterBuilder::new()
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_rich_accounts(1)
            .with_custom_contracts(vec![ContractToDeploy::new(
                precompile_contract.bytecode.to_vec(),
                precompiles_contract_address,
            )])
            .with_storage(storage.clone())
            .build::<VM>();

        if let Some(prewarmed_hash) = prewarmed_hash {
            let is_fresh = vm.vm.manually_decommit(prewarmed_hash);
            assert!(is_fresh, "{prewarmed_hash:?} should not be decommitted yet");
        }

        let account = &mut vm.rich_accounts[0];
        let tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(precompiles_contract_address),
                calldata: call_code_oracle_function
                    .encode_input(&[
                        Token::FixedBytes(queried_hash.0.to_vec()),
                        Token::FixedBytes(body_keccak_hash.to_vec()),
                    ])
                    .unwrap(),
                value: U256::zero(),
                factory_deps: vec![],
            },
            None,
        );
        vm.vm.push_transaction(tx);
        let result = vm.vm.execute(InspectExecutionMode::OneTx);
        assert!(
            !result.result.is_failed(),
            "`CodeOracle` call failed: {result:#?}"
        );
        let log =
            find_code_oracle_cost_log(precompiles_contract_address, &result.logs.storage_logs);
        h256_to_u256(log.log.value)
    };

    // `CodeOracle.yul` charges `4 * padded_len_in_words` for both versions.
    let extra_cost = U256::from(4 * (body.len() / 32));

    // Both orderings occur in production: native first via an `L1 -> L2` transaction's `factoryDeps`,
    // EVM first via `EvmEmulator::fetchBytecode` on any call into an EVM contract.
    for (queried_hash, alias_hash, label) in [
        (evm_hash, native_hash, "CodeOracle(evm) after native"),
        (native_hash, evm_hash, "CodeOracle(native) after evm"),
    ] {
        let when_fresh = measure_oracle_cost(queried_hash, None);
        let after_alias = measure_oracle_cost(queried_hash, Some(alias_hash));
        let after_same_hash = measure_oracle_cost(queried_hash, Some(queried_hash));

        assert_eq!(
            after_alias, after_same_hash,
            "{label}: the `decommit` extra cost ({extra_cost}) was charged again for the \
             cross-version alias"
        );
        // A genuinely first decommit still pays it, so the test cannot pass by refunding (or
        // charging) in both states.
        assert_eq!(
            when_fresh - after_alias,
            extra_cost,
            "{label}: a fresh decommit must cost exactly `extra_cost` more than a repeat"
        );
    }
}

/// The proving side decommits the bootloader before execution starts, so a `CodeOracle` call on the
/// bootloader hash must be a repeat and get the `decommit` extra cost refunded.
pub(crate) fn test_bootloader_in_code_oracle<VM: TestedVm>(store_bootloader: bool) {
    let precompiles_contract_address = Address::repeat_byte(1);
    let bootloader = default_system_env().base_system_smart_contracts.bootloader;
    // Same length as the bootloader, so only decommit freshness affects the cost difference.
    let mut other_body = bootloader.code.clone();
    *other_body.last_mut().unwrap() ^= 1;
    let other_hash = BytecodeHash::for_bytecode(&other_body).value();

    let mut storage = get_empty_storage();
    for hash in [bootloader.hash, other_hash] {
        storage.set_value(get_known_code_key(&hash), u256_to_h256(U256::one()));
    }
    storage.store_factory_dep(other_hash, other_body.clone());
    if store_bootloader {
        storage.store_factory_dep(bootloader.hash, bootloader.code.clone());
    } else {
        assert!(storage.load_factory_dep(bootloader.hash).is_none());
    }

    let precompile_contract = TestContract::precompiles_test();
    let call_code_oracle_function = precompile_contract.function("callCodeOracle");

    let measure_oracle_cost = |queried_hash: H256, body: &[u8]| -> U256 {
        let mut vm = VmTesterBuilder::new()
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_rich_accounts(1)
            .with_custom_contracts(vec![ContractToDeploy::new(
                precompile_contract.bytecode.to_vec(),
                precompiles_contract_address,
            )])
            .with_storage(storage.clone())
            .build::<VM>();

        let account = &mut vm.rich_accounts[0];
        let tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(precompiles_contract_address),
                calldata: call_code_oracle_function
                    .encode_input(&[
                        Token::FixedBytes(queried_hash.0.to_vec()),
                        Token::FixedBytes(keccak256(body).to_vec()),
                    ])
                    .unwrap(),
                value: U256::zero(),
                factory_deps: vec![],
            },
            None,
        );
        vm.vm.push_transaction(tx);
        let result = vm.vm.execute(InspectExecutionMode::OneTx);
        assert!(
            !result.result.is_failed(),
            "`CodeOracle` call failed: {result:#?}"
        );
        let log =
            find_code_oracle_cost_log(precompiles_contract_address, &result.logs.storage_logs);
        h256_to_u256(log.log.value)
    };

    let bootloader_cost = measure_oracle_cost(bootloader.hash, &bootloader.code);
    let other_cost = measure_oracle_cost(other_hash, &other_body);
    let extra_cost = U256::from(4 * (bootloader.code.len() / 32));
    assert_eq!(other_cost - bootloader_cost, extra_cost);
}

pub(crate) fn test_bootloader_in_create2<VM: TestedVm>() {
    let bootloader = default_system_env().base_system_smart_contracts.bootloader;
    let mut other_body = bootloader.code.clone();
    *other_body.last_mut().unwrap() ^= 1;
    let other_hash = BytecodeHash::for_bytecode(&other_body).value();

    let mut storage = get_empty_storage();
    for hash in [bootloader.hash, other_hash] {
        storage.set_value(get_known_code_key(&hash), u256_to_h256(U256::one()));
    }
    storage.store_factory_dep(other_hash, other_body.clone());
    assert!(storage.load_factory_dep(bootloader.hash).is_none());

    let measure_deployment_cost = |body: Vec<u8>, prewarm: bool| {
        let mut vm = VmTesterBuilder::new()
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_rich_accounts(1)
            .with_storage(storage.clone())
            .build::<VM>();
        let (mut execute, params) = Execute::for_create2_deploy(H256::zero(), body, &[]);
        execute.factory_deps.clear();
        if prewarm {
            assert!(vm.vm.manually_decommit(params.bytecode_hash));
        }
        let tx = vm.rich_accounts[0].get_l2_tx_for_execute(execute, None);
        vm.vm.push_transaction(tx);
        let result = vm.vm.execute(InspectExecutionMode::OneTx);
        assert!(
            matches!(result.result, ExecutionResult::Revert { .. }),
            "{result:#?}"
        );
        assert!(vm
            .vm
            .decommitted_hashes()
            .contains(&h256_to_u256(params.bytecode_hash)));
        result.statistics.gas_used
    };

    let bootloader_cost = measure_deployment_cost(bootloader.code.clone(), false);
    let other_cost = measure_deployment_cost(other_body.clone(), false);
    let prewarmed_cost = measure_deployment_cost(other_body, true);
    assert_eq!(bootloader_cost, prewarmed_cost);
    let extra_cost = 4 * (bootloader.code.len() / 32) as u64;
    assert_eq!(other_cost - bootloader_cost, extra_cost);
}
