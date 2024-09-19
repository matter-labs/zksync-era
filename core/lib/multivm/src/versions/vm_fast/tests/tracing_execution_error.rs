use zksync_types::{Execute, H160};

use crate::{
    interface::{TxExecutionMode, TxRevertReason, VmRevertReason},
    vm_fast::tests::{
        tester::{ExpectedError, TransactionTestInfo, VmTesterBuilder},
        utils::{get_execute_error_calldata, read_error_contract, BASE_SYSTEM_CONTRACTS},
    },
};

#[test]
fn test_tracing_of_execution_errors() {
    let contract_address = H160::random();
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_custom_contracts(vec![(read_error_contract(), contract_address, false)])
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_deployer()
        .with_random_rich_accounts(1)
        .build();

    let account = &mut vm.rich_accounts[0];

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(contract_address),
            calldata: get_execute_error_calldata(),
            value: Default::default(),
            factory_deps: vec![],
        },
        None,
    );

    vm.execute_tx_and_verify(TransactionTestInfo::new_rejected(
        tx,
        ExpectedError {
            revert_reason: TxRevertReason::TxReverted(VmRevertReason::General {
                msg: "short".to_string(),
                data: vec![
                    8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 115, 104, 111, 114, 116,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0,
                ],
            }),
            modifier: None,
        },
    ));
}
