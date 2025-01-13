//! Call tracer tests. These tests are special in the sense that it's too unreliable to keep fixtures
//! (since they can be invalidated by unrelated changes in system contracts, e.g. by changing consumed gas costs).

use zksync_test_contracts::TestContract;
use zksync_types::{zk_evm_types::FarCallOpcode, Address, Execute};

use crate::{
    interface::{Call, CallType, TxExecutionMode},
    versions::testonly::{ContractToDeploy, TestedVmWithCallTracer, VmTester, VmTesterBuilder},
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};

fn check_call(call: &Call) {
    assert!(call.gas_used < call.gas);
    assert!(call.gas_used > call.calls.iter().map(|call| call.gas_used).sum::<u64>());

    for subcall in &call.calls {
        if subcall.r#type != CallType::Call(FarCallOpcode::Mimic) {
            pretty_assertions::assert_eq!(call.to, subcall.from);
        }
        check_call(subcall);
    }
}

pub(crate) fn test_basic_behavior<VM: TestedVmWithCallTracer>() {
    let bytecode = TestContract::counter().bytecode.to_vec();
    let address = Address::repeat_byte(0xA5);
    let mut vm: VmTester<VM> = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::new(bytecode, address)])
        .build();

    let calldata = "7cf5dab00000000000000000000000000000000000000000000000000000000000000006";
    let calldata = hex::decode(calldata).unwrap();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: calldata.clone(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(tx);
    let (res, call_traces) = vm.vm.inspect_with_call_tracer();
    assert!(!res.result.is_failed(), "{:#?}", res.result);

    for call in &call_traces {
        check_call(call);
        assert_eq!(call.error, None);
        assert_eq!(call.revert_reason, None);
    }
    // There should be a call to the contract at almost the top level.
    let calls_to_contract: Vec<_> = call_traces
        .iter()
        .flat_map(|call| call.calls.iter().chain([call]))
        .filter(|call| call.to == address)
        .collect();
    assert_eq!(calls_to_contract.len(), 1);
    let call_to_contract = *calls_to_contract.first().unwrap();
    assert_eq!(call_to_contract.from, account.address);
    assert_eq!(call_to_contract.input, calldata);
}
