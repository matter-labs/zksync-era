use zksync_types::{Address, Execute, U256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        constants::BLOCK_GAS_LIMIT,
        tests::tester::VmTesterBuilder,
        tracers::circuits_capacity::{GEOMETRY_CONFIG, OVERESTIMATE_PERCENT},
        HistoryEnabled,
    },
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
    let res = vm
        .vm
        .inspect_with_circuit_statistic(Default::default(), VmExecutionMode::OneTx);

    const EXPECTED_CIRCUITS_USED: f32 = 4.8685;
    let delta =
        (res.statistics.estimated_circuits_used - EXPECTED_CIRCUITS_USED) / EXPECTED_CIRCUITS_USED;

    if delta.abs() > 0.1 {
        panic!(
            "Estimation differs from expected result by too much: {}%, expected value: {}",
            delta * 100.0,
            res.statistics.estimated_circuits_used
        );
    }

    // Check consistency between `estimated_circuits_used` and `circuit_statistic`.
    let statistic = res.statistics.circuit_statistic.unwrap();
    let delta = (statistic.total(&GEOMETRY_CONFIG) * OVERESTIMATE_PERCENT
        - res.statistics.estimated_circuits_used)
        / res.statistics.estimated_circuits_used;
    if delta > 0.01 {
        panic!(
            "`circuit_statistic` and `estimated_circuits_used` are inconsistent, difference {}%",
            delta * 100.0,
        );
    }

    // Check `circuit_statistic` further.
    assert!(statistic.main_vm > 0);
    assert!(statistic.ram_permutation > 0);
    assert!(statistic.storage_application_by_reads > 0);
    assert!(statistic.storage_application_by_writes > 0);
    assert!(statistic.storage_sorter > 0);
    assert!(statistic.code_decommitter > 0);
    assert!(statistic.code_decommitter_sorter > 0);
    assert!(statistic.log_demuxer > 0);
    assert!(statistic.events_sorter > 0);
    assert!(statistic.keccak256 > 0);
    // Ecrecover should be used to validata tx signature.
    assert_eq!(statistic.ecrecover, 1);
    // SHA256 shouldn't be used.
    assert_eq!(statistic.sha256, 0);
}
