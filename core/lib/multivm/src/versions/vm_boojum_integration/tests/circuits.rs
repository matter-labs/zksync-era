use zkevm_test_harness_1_4_0::geometry_config::get_geometry_config;
use zksync_types::{Address, Execute, U256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_boojum_integration::{constants::BLOCK_GAS_LIMIT, tests::tester::VmTesterBuilder, HistoryEnabled},
};

// Checks that estimated number of circuits for simple transfer doesn't differ much
// from hardcoded expected value.
#[test]
fn test_circuits() {
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_gas_limit(BLOCK_GAS_LIMIT)
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

    let statistic = res.statistics.circuit_statistic;
    // Check `circuit_statistic`.
    assert!(statistic.main_vm > f32::EPSILON);
    assert!(statistic.ram_permutation > f32::EPSILON);
    assert!(statistic.storage_application > f32::EPSILON);
    assert!(statistic.storage_sorter > f32::EPSILON);
    assert!(statistic.code_decommitter > f32::EPSILON);
    assert!(statistic.code_decommitter_sorter > f32::EPSILON);
    assert!(statistic.log_demuxer > f32::EPSILON);
    assert!(statistic.events_sorter > f32::EPSILON);
    assert!(statistic.keccak256 > f32::EPSILON);
    // Single `ecrecover` should be used to validate tx signature.
    assert_eq!(
        statistic.ecrecover,
        1.0 / get_geometry_config().cycles_per_ecrecover_circuit as f32
    );
    // `sha256` shouldn't be used.
    assert_eq!(statistic.sha256, 0.0);

    const EXPECTED_CIRCUITS_USED: f32 = 4.6363;
    let delta = (statistic.total_f32() - EXPECTED_CIRCUITS_USED) / EXPECTED_CIRCUITS_USED;

    if delta.abs() > 0.1 {
        panic!(
            "Estimation differs from expected result by too much: {}%, expected value: {}, got {}",
            delta * 100.0,
            EXPECTED_CIRCUITS_USED,
            statistic.total_f32(),
        );
    }
}
