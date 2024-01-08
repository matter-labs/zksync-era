use zksync_types::{Address, Execute, U256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{constants::BLOCK_GAS_LIMIT, tests::tester::VmTesterBuilder, HistoryEnabled},
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
}
