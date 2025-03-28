use circuit_sequencer_api::geometry_config::ProtocolGeometry;
use zksync_types::Execute;

use crate::{
    interface::{InspectExecutionMode, TxExecutionMode, VmInterface, VmInterfaceExt},
    versions::testonly::{
        precompiles::{test_ecrecover, test_keccak, test_sha256},
        VmTesterBuilder,
    },
    vm_fast::Vm,
};

#[test]
fn keccak() {
    test_keccak::<Vm<_>>();
}

#[test]
fn sha256() {
    test_sha256::<Vm<_>>();
}

#[test]
fn ecrecover() {
    test_ecrecover::<Vm<_>>();
}

#[test]
fn caching_ecrecover_result() {
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_rich_accounts(1)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<Vm<_>>();
    vm.vm.skip_signature_verification();

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

    assert!(vm.vm.world.precompiles.expected_ecrecover_call.is_some());
    assert_eq!(vm.vm.world.precompiles.expected_calls.get(), 0);

    let exec_result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");
    assert_eq!(vm.vm.world.precompiles.expected_calls.get(), 1);

    // Cycle stats should still be produced for the cached call
    let ecrecover_count = exec_result.statistics.circuit_statistic.ecrecover
        * ProtocolGeometry::V1_5_0
            .config()
            .cycles_per_ecrecover_circuit as f32;
    assert!((ecrecover_count - 1.0).abs() < 1e-4, "{ecrecover_count}");
}
