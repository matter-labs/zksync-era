//! Call tracer tests. These tests are special in the sense that it's too unreliable to keep fixtures
//! (since they can be invalidated by unrelated changes in system contracts, e.g. by changing consumed gas costs).

use assert_matches::assert_matches;
use ethabi::Token;
use zksync_system_constants::MSG_VALUE_SIMULATOR_ADDRESS;
use zksync_test_contracts::{Account, LoadnextContractExecutionParams, TestContract, TxType};
use zksync_types::{
    fee::Fee, utils::deployed_address_create, zk_evm_types::FarCallOpcode, Address, Execute,
};

use super::{ContractToDeploy, TestedVmWithCallTracer, VmTester, VmTesterBuilder};
use crate::{
    interface::{Call, CallType, ExecutionResult, TxExecutionMode},
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

fn extract_single_call(calls: &[Call], filter: impl Fn(&Call) -> bool) -> &Call {
    fn walk<'a>(
        matching_call: &mut Option<&'a Call>,
        calls: &'a [Call],
        filter: &impl Fn(&Call) -> bool,
    ) {
        for call in calls {
            if filter(call) {
                if let Some(prev_call) = matching_call {
                    panic!("Multiple call match filter: {prev_call:?}, {call:?}");
                }
                *matching_call = Some(call);
            }
            walk(matching_call, &call.calls, filter);
        }
    }

    let mut matching_call = None;
    walk(&mut matching_call, calls, &filter);
    matching_call.expect("no calls match the filter")
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

    let call_to_contract = extract_single_call(&call_traces, |call| call.to == address);
    assert_eq!(call_to_contract.from, account.address);
    assert_eq!(call_to_contract.input, calldata);
}

pub(crate) fn test_transfer<VM: TestedVmWithCallTracer>() {
    let mut vm: VmTester<VM> = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    let recipient = Address::repeat_byte(0x23);
    let value = 1_000_000_000.into();
    let account = &mut vm.rich_accounts[0];
    let transfer = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(recipient),
            calldata: vec![],
            value,
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(transfer);
    let (res, call_traces) = vm.vm.inspect_with_call_tracer();
    assert!(!res.result.is_failed(), "{:#?}", res.result);

    for call in &call_traces {
        check_call(call);
        assert_eq!(call.error, None);
        assert_eq!(call.revert_reason, None);
    }

    let transfer_call = extract_single_call(&call_traces, |call| call.to == recipient);
    assert_eq!(transfer_call.from, account.address);
    assert_eq!(transfer_call.value, value);
}

pub(crate) fn test_reverted_tx<VM: TestedVmWithCallTracer>() {
    let counter_address = Address::repeat_byte(0x23);
    let mut vm: VmTester<VM> = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::new(
            TestContract::counter().bytecode.to_vec(),
            counter_address,
        )])
        .build();

    let account = &mut vm.rich_accounts[0];
    let calldata = TestContract::counter()
        .function("incrementWithRevert")
        .encode_input(&[Token::Uint(1.into()), Token::Bool(true)])
        .unwrap();
    let reverted_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(counter_address),
            calldata,
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(reverted_tx);
    let (res, call_traces) = vm.vm.inspect_with_call_tracer();
    assert_matches!(&res.result, ExecutionResult::Revert { .. });

    let call_to_contract = extract_single_call(&call_traces, |call| call.to == counter_address);
    assert_eq!(
        call_to_contract.revert_reason.as_ref().unwrap(),
        "This method always reverts"
    );
}

pub(crate) fn test_out_of_gas<VM: TestedVmWithCallTracer>() {
    let contract_address = Address::repeat_byte(0x23);
    let mut vm: VmTester<VM> = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_rich_accounts(1)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::new(
            TestContract::expensive().bytecode.to_vec(),
            contract_address,
        )])
        .build();

    let account = &mut vm.rich_accounts[0];
    let execute = Execute {
        contract_address: Some(contract_address),
        calldata: TestContract::expensive()
            .function("expensive")
            .encode_input(&[Token::Uint(1_000.into())])
            .unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    };
    let out_of_gas_tx = account.get_l2_tx_for_execute(
        execute,
        Some(Fee {
            gas_limit: 500_000.into(), // insufficient gas
            ..Account::default_fee()
        }),
    );

    vm.vm.push_transaction(out_of_gas_tx);
    let (res, call_traces) = vm.vm.inspect_with_call_tracer();
    assert_matches!(&res.result, ExecutionResult::Revert { .. });

    let out_of_gas_call = extract_single_call(&call_traces, |call| {
        call.from == account.address && call.to == contract_address
    });
    assert_eq!(out_of_gas_call.error.as_ref().unwrap(), "Panic");
    assert_eq!(out_of_gas_call.gas_used, out_of_gas_call.gas);

    let parent_call =
        extract_single_call(&call_traces, |call| call.calls.contains(out_of_gas_call));
    assert_eq!(
        parent_call.revert_reason.as_ref().unwrap(),
        "Unknown revert reason"
    );
}

pub(crate) fn test_reverted_deployment_tx<VM: TestedVmWithCallTracer>() {
    let mut vm: VmTester<VM> = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_rich_accounts(1)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    let account = &mut vm.rich_accounts[0];
    let deploy_tx = account.get_deploy_tx(TestContract::failed_call().bytecode, None, TxType::L2);

    vm.vm.push_transaction(deploy_tx.tx);
    let (res, call_traces) = vm.vm.inspect_with_call_tracer();
    assert_matches!(&res.result, ExecutionResult::Success { .. });

    let constructor_call = extract_single_call(&call_traces, |call| {
        call.r#type == CallType::Create && call.from == account.address
    });
    assert_eq!(constructor_call.input, [] as [u8; 0]);
    assert_eq!(constructor_call.error, None);
    assert_eq!(constructor_call.revert_reason, None);
    let deploy_address = deployed_address_create(account.address, 0.into());
    assert_eq!(constructor_call.to, deploy_address);

    assert_eq!(constructor_call.calls.len(), 1, "{constructor_call:#?}");
    let inner_call = &constructor_call.calls[0];
    assert_eq!(inner_call.from, deploy_address);
    assert_eq!(inner_call.to, MSG_VALUE_SIMULATOR_ADDRESS);
    inner_call.revert_reason.as_ref().unwrap();
}

pub(crate) fn test_recursive_tx<VM: TestedVmWithCallTracer>() {
    let contract_address = Address::repeat_byte(0x42);
    let mut vm: VmTester<VM> = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::new(
            TestContract::load_test().bytecode.to_vec(),
            contract_address,
        )])
        .build();

    let account = &mut vm.rich_accounts[0];
    let calldata = LoadnextContractExecutionParams {
        recursive_calls: 20,
        ..LoadnextContractExecutionParams::empty()
    }
    .to_bytes();
    let recursive_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(contract_address),
            calldata: calldata.clone(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(recursive_tx);
    let (res, call_traces) = vm.vm.inspect_with_call_tracer();
    assert!(!res.result.is_failed(), "{:#?}", res.result);

    let mut call_to_contract = extract_single_call(&call_traces, |call| {
        call.to == contract_address && call.input == calldata
    });
    let mut depth = 0;
    while let Some(child_call) = call_to_contract.calls.first() {
        assert_eq!(call_to_contract.calls.len(), 1, "{call_to_contract:#?}");
        assert_eq!(child_call.from, contract_address);
        assert_eq!(child_call.to, contract_address);
        assert_ne!(child_call.input, call_to_contract.input);

        depth += 1;
        call_to_contract = child_call;
    }
    assert_eq!(depth, 20);
}
