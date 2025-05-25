use ethabi::Token;
use zksync_test_contracts::{
    DeployContractsTx, LoadnextContractExecutionParams, TestContract, TxType,
};
use zksync_types::{get_nonce_key, U256};
use zksync_vm_interface::InspectExecutionMode;

use super::TestedLatestVm;
use crate::{
    interface::{
        storage::WriteStorage,
        tracer::{TracerExecutionStatus, TracerExecutionStopReason},
        TxExecutionMode, VmInterface, VmInterfaceExt, VmInterfaceHistoryEnabled,
    },
    tracers::dynamic::vm_1_5_2::DynTracer,
    versions::testonly::{
        rollbacks::{test_rollback_in_call_mode, test_vm_loadnext_rollbacks, test_vm_rollbacks},
        VmTesterBuilder,
    },
    vm_latest::{
        bootloader::BootloaderState, types::ZkSyncVmState, HistoryEnabled, HistoryMode,
        SimpleMemory, ToTracerPointer, Vm, VmTracer,
    },
};

#[test]
fn vm_rollbacks() {
    test_vm_rollbacks::<Vm<_, HistoryEnabled>>();
}

#[test]
fn vm_loadnext_rollbacks() {
    test_vm_loadnext_rollbacks::<Vm<_, HistoryEnabled>>();
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
fn layered_rollback() {
    // This test checks that the layered rollbacks work correctly, i.e.
    // the rollback by the operator will always revert all the changes

    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<TestedLatestVm>();

    let account = &mut vm.rich_accounts[0];

    let DeployContractsTx {
        tx: deploy_tx,
        address,
        ..
    } = account.get_deploy_tx(
        TestContract::load_test().bytecode,
        Some(&[Token::Uint(0.into())]),
        TxType::L2,
    );
    vm.vm.push_transaction(deploy_tx);
    let deployment_res = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!deployment_res.result.is_failed(), "transaction failed");

    let loadnext_transaction = account.get_loadnext_transaction(
        address,
        LoadnextContractExecutionParams {
            initial_writes: 1,
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
    let tracer = MaxRecursionTracer {
        max_recursion_depth: 15,
    }
    .into_tracer_pointer();
    vm.vm
        .inspect(&mut tracer.into(), InspectExecutionMode::OneTx);

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
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "transaction must not fail");
}

#[test]
fn rollback_in_call_mode() {
    test_rollback_in_call_mode::<Vm<_, HistoryEnabled>>();
}
