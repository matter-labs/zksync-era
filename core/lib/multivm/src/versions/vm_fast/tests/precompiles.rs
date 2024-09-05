use circuit_sequencer_api_1_5_0::geometry_config::get_geometry_config;
use zksync_types::{Address, Execute};

use super::{tester::VmTesterBuilder, utils::read_precompiles_contract};
use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    versions::testonly::ContractToDeploy,
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};

#[test]
fn test_keccak() {
    // Execute special transaction and check that at least 1000 keccak calls were made.
    let contract = read_precompiles_contract();
    let address = Address::random();
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::account(contract, address)])
        .build();

    // calldata for `doKeccak(1000)`.
    let keccak1000_calldata =
        "370f20ac00000000000000000000000000000000000000000000000000000000000003e8";

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: address,
            calldata: hex::decode(keccak1000_calldata).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let exec_result = vm.vm.inspect((), VmExecutionMode::OneTx);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let keccak_count = exec_result.statistics.circuit_statistic.keccak256
        * get_geometry_config().cycles_per_keccak256_circuit as f32;
    assert!(keccak_count >= 1000.0, "{keccak_count}");
}

#[test]
fn test_sha256() {
    // Execute special transaction and check that at least 1000 `sha256` calls were made.
    let contract = read_precompiles_contract();
    let address = Address::random();
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::account(contract, address)])
        .build();

    // calldata for `doSha256(1000)`.
    let sha1000_calldata =
        "5d0b4fb500000000000000000000000000000000000000000000000000000000000003e8";

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: address,
            calldata: hex::decode(sha1000_calldata).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let exec_result = vm.vm.inspect((), VmExecutionMode::OneTx);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let sha_count = exec_result.statistics.circuit_statistic.sha256
        * get_geometry_config().cycles_per_sha256_circuit as f32;
    assert!(sha_count >= 1000.0, "{sha_count}");
}

#[test]
fn test_ecrecover() {
    // Execute simple transfer and check that exactly 1 `ecrecover` call was made (it's done during tx validation).
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: account.address,
            calldata: vec![],
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let exec_result = vm.vm.inspect((), VmExecutionMode::OneTx);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let ecrecover_count = exec_result.statistics.circuit_statistic.ecrecover
        * get_geometry_config().cycles_per_ecrecover_circuit as f32;
    assert!((ecrecover_count - 1.0).abs() < 1e-4, "{ecrecover_count}");
}
