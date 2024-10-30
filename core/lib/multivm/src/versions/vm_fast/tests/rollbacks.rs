use assert_matches::assert_matches;
use ethabi::Token;
use zksync_contracts::{get_loadnext_contract, test_contracts::LoadnextContractExecutionParams};
use zksync_types::{Address, Execute, U256};
use zksync_vm_interface::VmInterfaceExt;

use crate::{
    interface::{ExecutionResult, TxExecutionMode},
    versions::testonly::ContractToDeploy,
    vm_fast::tests::{
        tester::{DeployContractsTx, TransactionTestInfo, TxModifier, TxType, VmTesterBuilder},
        utils::read_test_contract,
    },
};

#[test]
fn test_vm_rollbacks() {
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let mut account = vm.rich_accounts[0].clone();
    let counter = read_test_contract();
    let tx_0 = account.get_deploy_tx(&counter, None, TxType::L2).tx;
    let tx_1 = account.get_deploy_tx(&counter, None, TxType::L2).tx;
    let tx_2 = account.get_deploy_tx(&counter, None, TxType::L2).tx;

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
        TransactionTestInfo::new_rejected(tx_2.clone(), TxModifier::WrongNonce.into()),
        // This tx will succeed
        TransactionTestInfo::new_processed(tx_0.clone(), false),
        // The correct nonce is 1, this tx will fail
        TransactionTestInfo::new_rejected(tx_0.clone(), TxModifier::NonceReused.into()),
        // The correct nonce is 1, this tx will fail
        TransactionTestInfo::new_rejected(tx_2.clone(), TxModifier::WrongNonce.into()),
        // This tx will succeed
        TransactionTestInfo::new_processed(tx_1, false),
        // The correct nonce is 2, this tx will fail
        TransactionTestInfo::new_rejected(tx_0.clone(), TxModifier::NonceReused.into()),
        // This tx will succeed
        TransactionTestInfo::new_processed(tx_2.clone(), false),
        // This tx will fail
        TransactionTestInfo::new_rejected(tx_2, TxModifier::NonceReused.into()),
        TransactionTestInfo::new_rejected(tx_0, TxModifier::NonceReused.into()),
    ]);

    pretty_assertions::assert_eq!(result_without_rollbacks, result_with_rollbacks);
}

#[test]
fn test_vm_loadnext_rollbacks() {
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();
    let mut account = vm.rich_accounts[0].clone();

    let loadnext_contract = get_loadnext_contract();
    let loadnext_constructor_data = &[Token::Uint(U256::from(100))];
    let DeployContractsTx {
        tx: loadnext_deploy_tx,
        address,
        ..
    } = account.get_deploy_tx_with_factory_deps(
        &loadnext_contract.bytecode,
        Some(loadnext_constructor_data),
        loadnext_contract.factory_deps.clone(),
        TxType::L2,
    );

    let loadnext_tx_1 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: LoadnextContractExecutionParams {
                reads: 100,
                writes: 100,
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
                writes: 100,
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
            TxModifier::NonceReused.into(),
        ),
        TransactionTestInfo::new_processed(loadnext_tx_1, false),
        TransactionTestInfo::new_processed(loadnext_tx_2.clone(), true),
        TransactionTestInfo::new_processed(loadnext_tx_2.clone(), true),
        TransactionTestInfo::new_rejected(loadnext_deploy_tx, TxModifier::NonceReused.into()),
        TransactionTestInfo::new_processed(loadnext_tx_2, false),
    ]);

    assert_eq!(result_without_rollbacks, result_with_rollbacks);
}

#[test]
fn rollback_in_call_mode() {
    let counter_bytecode = read_test_contract();
    let counter_address = Address::repeat_byte(1);

    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::EthCall)
        .with_custom_contracts(vec![ContractToDeploy::new(
            counter_bytecode,
            counter_address,
        )])
        .with_random_rich_accounts(1)
        .build();
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
    assert_eq!(vm_result.logs.storage_logs, []);
}
