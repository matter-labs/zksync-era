use ethabi::Token;
use zksync_contracts::{get_loadnext_contract, test_contracts::LoadnextContractExecutionParams};
use zksync_types::{get_nonce_key, Execute, U256};

use crate::{
    interface::{
        storage::WriteStorage,
        tracer::{TracerExecutionStatus, TracerExecutionStopReason},
        TxExecutionMode, VmExecutionMode, VmInterface, VmInterfaceExt, VmInterfaceHistoryEnabled,
    },
    tracers::dynamic::vm_1_5_0::DynTracer,
    vm_latest::{
        tests::{
            tester::{DeployContractsTx, TransactionTestInfo, TxModifier, TxType, VmTesterBuilder},
            utils::read_test_contract,
        },
        types::internals::ZkSyncVmState,
        BootloaderState, HistoryEnabled, HistoryMode, SimpleMemory, ToTracerPointer, VmTracer,
    },
};

#[test]
fn test_vm_rollbacks() {
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
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

    assert_eq!(result_without_rollbacks, result_with_rollbacks);
}

#[test]
fn test_vm_loadnext_rollbacks() {
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
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
            contract_address: address,
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
            contract_address: address,
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

// Testing tracer that does not allow the recursion to go deeper than a certain limit
struct MaxRecursionTracer {
    max_recursion_depth: usize,
}

/// Tracer responsible for calculating the number of storage invocations and
/// stopping the VM execution if the limit is reached.
impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for MaxRecursionTracer {}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for MaxRecursionTracer {
    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        let current_depth = state.local_state.callstack.depth();

        if current_depth > self.max_recursion_depth {
            TracerExecutionStatus::Stop(TracerExecutionStopReason::Finish)
        } else {
            TracerExecutionStatus::Continue
        }
    }
}

#[test]
fn test_layered_rollback() {
    // This test checks that the layered rollbacks work correctly, i.e.
    // the rollback by the operator will always revert all the changes

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let account = &mut vm.rich_accounts[0];
    let loadnext_contract = get_loadnext_contract().bytecode;

    let DeployContractsTx {
        tx: deploy_tx,
        address,
        ..
    } = account.get_deploy_tx(
        &loadnext_contract,
        Some(&[Token::Uint(0.into())]),
        TxType::L2,
    );
    vm.vm.push_transaction(deploy_tx);
    let deployment_res = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!deployment_res.result.is_failed(), "transaction failed");

    let loadnext_transaction = account.get_loadnext_transaction(
        address,
        LoadnextContractExecutionParams {
            writes: 1,
            recursive_calls: 20,
            ..LoadnextContractExecutionParams::empty()
        },
        TxType::L2,
    );

    let nonce_val = vm
        .vm
        .state
        .storage
        .storage
        .read_from_storage(&get_nonce_key(&account.address));

    vm.vm.make_snapshot();

    vm.vm.push_transaction(loadnext_transaction.clone());
    vm.vm.inspect(
        MaxRecursionTracer {
            max_recursion_depth: 15,
        }
        .into_tracer_pointer()
        .into(),
        VmExecutionMode::OneTx,
    );

    let nonce_val2 = vm
        .vm
        .state
        .storage
        .storage
        .read_from_storage(&get_nonce_key(&account.address));

    // The tracer stopped after the validation has passed, so nonce has already been increased
    assert_eq!(nonce_val + U256::one(), nonce_val2, "nonce did not change");

    vm.vm.rollback_to_the_latest_snapshot();

    let nonce_val_after_rollback = vm
        .vm
        .state
        .storage
        .storage
        .read_from_storage(&get_nonce_key(&account.address));

    assert_eq!(
        nonce_val, nonce_val_after_rollback,
        "nonce changed after rollback"
    );

    vm.vm.push_transaction(loadnext_transaction);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "transaction must not fail");
}
