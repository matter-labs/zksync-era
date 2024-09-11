use zksync_types::{Address, Execute, U256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_1_4_2::{constants::BLOCK_GAS_LIMIT, tests::tester::VmTesterBuilder},
};

// Checks that estimated number of circuits for simple transfer doesn't differ much
// from hardcoded expected value.
#[test]
fn test_circuits() {
    let mut vm = VmTesterBuilder::new(crate::vm_latest::HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_bootloader_gas_limit(BLOCK_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Address::random(),
            calldata: Vec::new(),
            value: U256::from(1u8),
            factory_deps: None,
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let res = vm.vm.inspect(Default::default(), VmExecutionMode::OneTx);

    let s = res.statistics.circuit_statistic;
    // Check `circuit_statistic`.
    const EXPECTED: [f32; 11] = [
        1.1979, 0.1390, 1.5455, 0.0031, 1.1799649, 0.00059, 0.003438, 0.00077, 0.1195, 0.1429, 0.0,
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
