use zksync_contracts::load_contract;
use zksync_types::{Address, Execute};

use super::{
    read_error_contract, tester::VmTesterBuilder, ContractToDeploy, TestedVm, BASE_SYSTEM_CONTRACTS,
};
use crate::{
    interface::{TxExecutionMode, TxRevertReason, VmRevertReason},
    versions::testonly::tester::{ExpectedError, TransactionTestInfo},
};

fn get_execute_error_calldata() -> Vec<u8> {
    let test_contract = load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/error/error.sol/SimpleRequire.json",
    );
    let function = test_contract.function("require_short").unwrap();
    function
        .encode_input(&[])
        .expect("failed to encode parameters")
}

pub(crate) fn test_tracing_of_execution_errors<VM: TestedVm>() {
    let contract_address = Address::repeat_byte(1);
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_custom_contracts(vec![ContractToDeploy::new(
            read_error_contract(),
            contract_address,
        )])
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

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
