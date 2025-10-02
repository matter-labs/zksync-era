use std::collections::HashMap;

use assert_matches::assert_matches;
use ethabi::Token;
use zksync_test_contracts::{
    DeployContractsTx, LoadnextContractExecutionParams, TestContract, TxType,
};
use zksync_types::{Address, Execute, Nonce, U256};

use super::{
    tester::{TransactionTestInfo, TxModifier, VmTesterBuilder},
    ContractToDeploy, TestedVm,
};
use crate::interface::{storage::ReadStorage, ExecutionResult, TxExecutionMode, VmInterfaceExt};

pub(crate) fn test_vm_rollbacks<VM: TestedVm>() {
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let mut account = vm.rich_accounts[0].clone();
    let counter = TestContract::counter().bytecode;
    let tx_0 = account.get_deploy_tx(counter, None, TxType::L2).tx;
    let tx_1 = account.get_deploy_tx(counter, None, TxType::L2).tx;
    let tx_2 = account.get_deploy_tx(counter, None, TxType::L2).tx;

    let result_without_rollbacks = vm.execute_and_verify_txs(&vec![
        TransactionTestInfo::new_processed(tx_0.clone(), false),
        TransactionTestInfo::new_processed(tx_1.clone(), false),
        TransactionTestInfo::new_processed(tx_2.clone(), false),
    ]);

    // reset vm
    vm.reset_with_empty_storage();

    let result_with_rollbacks = vm.execute_and_verify_txs(&vec![
        TransactionTestInfo::new_rejected(tx_0.clone(), TxModifier::WrongSignatureLength.into()),
        TransactionTestInfo::new_rejected(tx_0.clone(), TxModifier::WrongMagicValue.into()),
        TransactionTestInfo::new_rejected(tx_0.clone(), TxModifier::WrongSignature.into()),
        // The correct nonce is 0, this tx will fail
        TransactionTestInfo::new_rejected(
            tx_2.clone(),
            TxModifier::WrongNonce(tx_2.nonce().unwrap(), Nonce(0)).into(),
        ),
        // This tx will succeed
        TransactionTestInfo::new_processed(tx_0.clone(), false),
        // The correct nonce is 1, this tx will fail
        TransactionTestInfo::new_rejected(
            tx_0.clone(),
            TxModifier::NonceReused(tx_0.initiator_account(), tx_0.nonce().unwrap()).into(),
        ),
        // The correct nonce is 1, this tx will fail
        TransactionTestInfo::new_rejected(
            tx_2.clone(),
            TxModifier::WrongNonce(tx_2.nonce().unwrap(), Nonce(1)).into(),
        ),
        // This tx will succeed
        TransactionTestInfo::new_processed(tx_1, false),
        // The correct nonce is 2, this tx will fail
        TransactionTestInfo::new_rejected(
            tx_0.clone(),
            TxModifier::NonceReused(tx_0.initiator_account(), tx_0.nonce().unwrap()).into(),
        ),
        // This tx will succeed
        TransactionTestInfo::new_processed(tx_2.clone(), false),
        // This tx will fail
        TransactionTestInfo::new_rejected(
            tx_2.clone(),
            TxModifier::NonceReused(tx_2.initiator_account(), tx_2.nonce().unwrap()).into(),
        ),
        TransactionTestInfo::new_rejected(
            tx_0.clone(),
            TxModifier::NonceReused(tx_0.initiator_account(), tx_0.nonce().unwrap()).into(),
        ),
    ]);

    pretty_assertions::assert_eq!(result_without_rollbacks, result_with_rollbacks);
}

pub(crate) fn test_vm_loadnext_rollbacks<VM: TestedVm>() {
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();
    let mut account = vm.rich_accounts[0].clone();

    let loadnext_contract = TestContract::load_test();
    let loadnext_constructor_data = &[Token::Uint(U256::from(100))];
    let DeployContractsTx {
        tx: loadnext_deploy_tx,
        address,
        ..
    } = account.get_deploy_tx_with_factory_deps(
        loadnext_contract.bytecode,
        Some(loadnext_constructor_data),
        loadnext_contract.factory_deps(),
        TxType::L2,
    );

    let loadnext_tx_1 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: LoadnextContractExecutionParams {
                reads: 100,
                initial_writes: 100,
                repeated_writes: 100,
                events: 100,
                hashes: 500,
                recursive_calls: 10,
                deploys: 60,
            }
            .to_bytes(),
            value: Default::default(),
            factory_deps: vec![],
        },
        None,
    );

    let loadnext_tx_2 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: LoadnextContractExecutionParams {
                reads: 100,
                initial_writes: 100,
                repeated_writes: 100,
                events: 100,
                hashes: 500,
                recursive_calls: 10,
                deploys: 60,
            }
            .to_bytes(),
            value: Default::default(),
            factory_deps: vec![],
        },
        None,
    );

    let result_without_rollbacks = vm.execute_and_verify_txs(&vec![
        TransactionTestInfo::new_processed(loadnext_deploy_tx.clone(), false),
        TransactionTestInfo::new_processed(loadnext_tx_1.clone(), false),
        TransactionTestInfo::new_processed(loadnext_tx_2.clone(), false),
    ]);

    // reset vm
    vm.reset_with_empty_storage();

    let result_with_rollbacks = vm.execute_and_verify_txs(&vec![
        TransactionTestInfo::new_processed(loadnext_deploy_tx.clone(), false),
        TransactionTestInfo::new_processed(loadnext_tx_1.clone(), true),
        TransactionTestInfo::new_rejected(
            loadnext_deploy_tx.clone(),
            TxModifier::NonceReused(
                loadnext_deploy_tx.initiator_account(),
                loadnext_deploy_tx.nonce().unwrap(),
            )
            .into(),
        ),
        TransactionTestInfo::new_processed(loadnext_tx_1, false),
        TransactionTestInfo::new_processed(loadnext_tx_2.clone(), true),
        TransactionTestInfo::new_processed(loadnext_tx_2.clone(), true),
        TransactionTestInfo::new_rejected(
            loadnext_deploy_tx.clone(),
            TxModifier::NonceReused(
                loadnext_deploy_tx.initiator_account(),
                loadnext_deploy_tx.nonce().unwrap(),
            )
            .into(),
        ),
        TransactionTestInfo::new_processed(loadnext_tx_2, false),
    ]);

    assert_eq!(result_without_rollbacks, result_with_rollbacks);
}

pub(crate) fn test_rollback_in_call_mode<VM: TestedVm>() {
    let counter_bytecode = TestContract::counter().bytecode.to_vec();
    let counter_address = Address::repeat_byte(1);

    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::EthCall)
        .with_custom_contracts(vec![ContractToDeploy::new(
            counter_bytecode,
            counter_address,
        )])
        .with_rich_accounts(1)
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];
    let tx = account.get_test_contract_transaction(counter_address, true, None, false, TxType::L2);

    let (compression_result, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    compression_result.unwrap();
    assert_matches!(
        vm_result.result,
        ExecutionResult::Revert { output }
            if output.to_string().contains("This method always reverts")
    );

    let storage_logs = &vm_result.logs.storage_logs;
    let deduplicated_logs = storage_logs
        .iter()
        .filter_map(|log| log.log.is_write().then_some((log.log.key, log.log.value)));
    let deduplicated_logs: HashMap<_, _> = deduplicated_logs.collect();
    // Check that all storage changes are reverted
    let mut storage = vm.storage.borrow_mut();
    for (key, value) in deduplicated_logs {
        assert_eq!(storage.inner_mut().read_value(&key), value);
    }
}
