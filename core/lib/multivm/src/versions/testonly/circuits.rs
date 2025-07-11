use zksync_types::{Address, Execute, U256};

use super::tester::VmTesterBuilder;
use crate::{
    interface::{InspectExecutionMode, TxExecutionMode, VmInterfaceExt},
    versions::testonly::TestedVm,
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};

/// Checks that estimated number of circuits for simple transfer doesn't differ much
/// from hardcoded expected value.
pub(crate) fn test_circuits<VM: TestedVm>() {
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(Address::repeat_byte(1)),
            calldata: Vec::new(),
            value: U256::from(1u8),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!res.result.is_failed(), "{res:#?}");

    let s = res.statistics.circuit_statistic;
    // Check `circuit_statistic`.
    const EXPECTED: [f32; 13] = [
        1.258627,
        0.15830745,
        1.6666666,
        0.003154238,
        1.2084359,
        0.00058723404,
        0.0034893616,
        0.00076709175,
        0.11945392,
        0.14285715,
        0.0,
        0.0,
        0.0,
    ];
    let actual = [
        (s.main_vm, "main_vm"),
        (s.ram_permutation, "ram_permutation"),
        (s.storage_application, "storage_application"),
        (s.storage_sorter, "storage_sorter"),
        (s.code_decommitter, "code_decommitter"),
        (s.code_decommitter_sorter, "code_decommitter_sorter"),
        (s.log_demuxer, "log_demuxer"),
        (s.events_sorter, "events_sorter"),
        (s.keccak256, "keccak256"),
        (s.ecrecover, "ecrecover"),
        (s.sha256, "sha256"),
        (s.secp256k1_verify, "secp256k1_verify"),
        (s.transient_storage_checker, "transient_storage_checker"),
    ];
    for ((actual, name), expected) in actual.iter().zip(EXPECTED) {
        if expected == 0.0 {
            assert_eq!(
                *actual, expected,
                "Check failed for {}, expected {}, actual {}",
                name, expected, actual
            );
        } else {
            let diff = (actual - expected) / expected;
            assert!(
                diff.abs() < 0.1,
                "Check failed for {}, expected {}, actual {}",
                name,
                expected,
                actual
            );
        }
    }
}
