use crate::constants::BLOCK_GAS_LIMIT;
use crate::tests::tester::VmTesterBuilder;
use crate::tests::utils::read_max_depth_contract;
use crate::{CallTracer, HistoryEnabled, TxExecutionMode, VmExecutionMode};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use zksync_types::{Address, Execute};

// This test is ultra slow, so it's ignored by default.
#[test]
#[ignore]
fn test_max_depth() {
    let contarct = read_max_depth_contract();
    let address = Address::random();
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_gas_limit(BLOCK_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![(contarct, address, true)])
        .build();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: address,
            calldata: vec![],
            value: Default::default(),
            factory_deps: None,
        },
        None,
    );

    let result = Arc::new(OnceCell::new());
    let call_tracer = CallTracer::new(result.clone(), HistoryEnabled);
    vm.vm.push_transaction(tx);
    let res = vm
        .vm
        .inspect(vec![Box::new(call_tracer)], VmExecutionMode::OneTx);
    assert!(result.get().is_some());
    assert!(res.result.is_failed());
}
