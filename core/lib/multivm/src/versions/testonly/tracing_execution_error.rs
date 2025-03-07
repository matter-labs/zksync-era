use zksync_test_contracts::TestContract;
use zksync_types::{Address, Execute};

use super::{tester::VmTesterBuilder, ContractToDeploy, TestedVm, BASE_SYSTEM_CONTRACTS};
use crate::{
    interface::{TxExecutionMode, TxRevertReason, VmRevertReason},
    versions::testonly::tester::{ExpectedError, TransactionTestInfo},
};

pub(crate) fn test_tracing_of_execution_errors<VM: TestedVm>() {
    let contract_address = Address::repeat_byte(1);
    let bytecode = TestContract::reverts_test().bytecode.to_vec();
    let mut vm = VmTesterBuilder::new()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_custom_contracts(vec![ContractToDeploy::new(bytecode, contract_address)])
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];

    let require_fn = TestContract::reverts_test().function("require_short");
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(contract_address),
            calldata: require_fn.encode_input(&[]).unwrap(),
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
