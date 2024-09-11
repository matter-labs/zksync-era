use zk_evm_1_5_0::zk_evm_abstractions::precompiles::PrecompileAddress;
use zksync_types::{Address, Execute};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
        tests::{tester::VmTesterBuilder, utils::read_precompiles_contract},
        HistoryEnabled,
    },
};

#[test]
fn test_keccak() {
    // Execute special transaction and check that at least 1000 keccak calls were made.
    let contract = read_precompiles_contract();
    let address = Address::random();
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![(contract, address, true)])
        .build();

    // calldata for `doKeccak(1000)`.
    let keccak1000_calldata =
        "370f20ac00000000000000000000000000000000000000000000000000000000000003e8";

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: address,
            calldata: hex::decode(keccak1000_calldata).unwrap(),
            value: Default::default(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let _ = vm.vm.inspect(Default::default(), VmExecutionMode::OneTx);

    let keccak_count = vm
        .vm
        .state
        .precompiles_processor
        .precompile_cycles_history
        .inner()
        .iter()
        .filter(|(precompile, _)| precompile == &PrecompileAddress::Keccak256)
        .count();

    assert!(keccak_count >= 1000);
}

#[test]
fn test_sha256() {
    // Execute special transaction and check that at least 1000 `sha256` calls were made.
    let contract = read_precompiles_contract();
    let address = Address::random();
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![(contract, address, true)])
        .build();

    // calldata for `doSha256(1000)`.
    let sha1000_calldata =
        "5d0b4fb500000000000000000000000000000000000000000000000000000000000003e8";

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: address,
            calldata: hex::decode(sha1000_calldata).unwrap(),
            value: Default::default(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let _ = vm.vm.inspect(Default::default(), VmExecutionMode::OneTx);

    let sha_count = vm
        .vm
        .state
        .precompiles_processor
        .precompile_cycles_history
        .inner()
        .iter()
        .filter(|(precompile, _)| precompile == &PrecompileAddress::SHA256)
        .count();

    assert!(sha_count >= 1000);
}

#[test]
fn test_ecrecover() {
    // Execute simple transfer and check that exactly 1 `ecrecover` call was made (it's done during tx validation).
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
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
            calldata: Vec::new(),
            value: Default::default(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let _ = vm.vm.inspect(Default::default(), VmExecutionMode::OneTx);

    let ecrecover_count = vm
        .vm
        .state
        .precompiles_processor
        .precompile_cycles_history
        .inner()
        .iter()
        .filter(|(precompile, _)| precompile == &PrecompileAddress::Ecrecover)
        .count();

    assert_eq!(ecrecover_count, 1);
}
