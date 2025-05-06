use circuit_sequencer_api::geometry_config::ProtocolGeometry;
use zk_evm_1_5_2::zk_evm_abstractions::precompiles::PrecompileAddress;
use zksync_test_contracts::TestContract;
use zksync_types::{Address, Execute};

use super::{tester::VmTesterBuilder, TestedVm};
use crate::{
    interface::{InspectExecutionMode, TxExecutionMode, VmInterfaceExt},
    versions::testonly::ContractToDeploy,
    vm_latest::{
        constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
        old_vm::{
            history_recorder::HistoryDisabled, oracles::precompile::PrecompilesProcessorWithHistory,
        },
        MultiVmSubversion,
    },
};

pub(crate) fn test_keccak<VM: TestedVm>() {
    // Execute special transaction and check that at least 1000 keccak calls were made.
    let contract = TestContract::precompiles_test().bytecode.to_vec();
    let address = Address::repeat_byte(1);
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::account(contract, address)])
        .build::<VM>();

    // calldata for `doKeccak(1000)`.
    let keccak1000_calldata =
        "370f20ac00000000000000000000000000000000000000000000000000000000000003e8";

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: hex::decode(keccak1000_calldata).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let exec_result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let keccak_count = exec_result.statistics.circuit_statistic.keccak256
        * ProtocolGeometry::latest()
            .config()
            .cycles_per_keccak256_circuit as f32;
    assert!(keccak_count >= 1000.0, "{keccak_count}");
}

pub(crate) fn test_sha256<VM: TestedVm>() {
    // Execute special transaction and check that at least 1000 `sha256` calls were made.
    let contract = TestContract::precompiles_test().bytecode.to_vec();
    let address = Address::repeat_byte(1);
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::account(contract, address)])
        .build::<VM>();

    // calldata for `doSha256(1000)`.
    let sha1000_calldata =
        "5d0b4fb500000000000000000000000000000000000000000000000000000000000003e8";

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: hex::decode(sha1000_calldata).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let exec_result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let sha_count = exec_result.statistics.circuit_statistic.sha256
        * ProtocolGeometry::latest()
            .config()
            .cycles_per_sha256_circuit as f32;
    assert!(sha_count >= 1000.0, "{sha_count}");
}

pub(crate) fn test_ecrecover<VM: TestedVm>() {
    // Execute simple transfer and check that exactly 1 `ecrecover` call was made (it's done during tx validation).
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(account.address),
            calldata: vec![],
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let exec_result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let ecrecover_count = exec_result.statistics.circuit_statistic.ecrecover
        * ProtocolGeometry::latest()
            .config()
            .cycles_per_ecrecover_circuit as f32;
    assert!((ecrecover_count - 1.0).abs() < 1e-4, "{ecrecover_count}");
}

pub(crate) fn test_ecadd<VM: TestedVm>() {
    // Execute simple transfer and check that exactly 1 `ecadd` call was made (it's done during tx validation).
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<VM>();

    let ecadd_calldata = "0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002";
    let address = Address::from_low_u64_be(6);

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: hex::decode(ecadd_calldata).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let exec_result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let ecadd_count = exec_result.statistics.circuit_statistic.ecadd
        * ProtocolGeometry::latest().config().cycles_per_ecadd_circuit as f32;
    assert!(ecadd_count >= 0.001, "{ecadd_count}");
}

pub(crate) fn test_ecmul<VM: TestedVm>() {
    // Execute simple transfer and check that exactly 1 `ecmul` call was made (it's done during tx validation).
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<VM>();

    let ecmul_calldata = "0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000230644e72e131a029b85045b68181585d2833e84879b9709143e1f593f00000000000000000000000000000000000000000000000000000000000000000000000";
    let address = Address::from_low_u64_be(7);

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: hex::decode(ecmul_calldata).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let exec_result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let ecmul_count = exec_result.statistics.circuit_statistic.ecmul
        * ProtocolGeometry::latest().config().cycles_per_ecmul_circuit as f32;
    assert!(ecmul_count >= 0.001, "{ecmul_count}");
}

pub(crate) fn test_ecpairing<VM: TestedVm>() {
    // Execute simple transfer and check that exactly 1 `ecpairing` call was made (it's done during tx validation).
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<VM>();

    let ecpairing_calldata = "105456a333e6d636854f987ea7bb713dfd0ae8371a72aea313ae0c32c0bf10160cf031d41b41557f3e7e3ba0c51bebe5da8e6ecd855ec50fc87efcdeac168bcc0476be093a6d2b4bbf907172049874af11e1b6267606e00804d3ff0037ec57fd3010c68cb50161b7d1d96bb71edfec9880171954e56871abf3d93cc94d745fa114c059d74e5b6c4ec14ae5864ebe23a71781d86c29fb8fb6cce94f70d3de7a2101b33461f39d9e887dbb100f170a2345dde3c07e256d1dfa2b657ba5cd030427000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000021a2c3013d2ea92e13c800cde68ef56a294b883f6ac35d25f587c09b1b3c635f7290158a80cd3d66530f74dc94c94adb88f5cdb481acca997b6e60071f08a115f2f997f3dbd66a7afe07fe7862ce239edba9e05c5afff7f8a1259c9733b2dfbb929d1691530ca701b4a106054688728c9972c8512e9789e9567aae23e302ccd75";
    let address = Address::from_low_u64_be(8);

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: hex::decode(ecpairing_calldata).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let exec_result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let ecpairing_count = exec_result.statistics.circuit_statistic.ecpairing
        * ProtocolGeometry::latest()
            .config()
            .cycles_per_ecpairing_circuit as f32;
    assert!(ecpairing_count >= 0.001, "{ecpairing_count}");
}

pub(crate) fn test_modexp<VM: TestedVm>() {
    // Execute simple transfer and check that exactly 1 `modexp` call was made (it's done during tx validation).
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<VM>();

    let modexp_calldata = "000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000100";
    let address = Address::from_low_u64_be(5);

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: hex::decode(modexp_calldata).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let exec_result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let modexp_count = exec_result.statistics.circuit_statistic.modexp
        * ProtocolGeometry::latest()
            .config()
            .cycles_per_modexp_circuit as f32;
    assert!(modexp_count >= 0.001, "{modexp_count}");
}

pub(crate) fn test_v28_precompiles_disabled<VM: TestedVm>() {
    // Build the VM environment with a version lower than EcPrecompiles
    let processor =
        PrecompilesProcessorWithHistory::<HistoryDisabled>::new(MultiVmSubversion::EvmEmulator);

    // List of v28 precompile addresses to check
    let v28_precompile_addresses = [
        PrecompileAddress::Modexp as u16,
        PrecompileAddress::ECAdd as u16,
        PrecompileAddress::ECMul as u16,
        PrecompileAddress::ECPairing as u16,
    ];

    // Check that each v28 precompile address resolves to None
    for &address in &v28_precompile_addresses {
        let resolved = processor.resolve_precompile_address(address);
        assert!(
            resolved.is_none(),
            "Expected None for precompile address {address}, got {:?}",
            resolved
        );
    }
}
