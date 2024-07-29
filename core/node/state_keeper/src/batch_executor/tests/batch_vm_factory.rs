use zksync_multivm::{
    interface::{FinishedL1Batch, L1BatchEnv, L2BlockEnv, SystemEnv, VmFactory},
    vm_latest::HistoryEnabled,
    VmInstance,
};
use zksync_state::{ReadStorage, StoragePtr, StorageView};
use zksync_types::Transaction;

use crate::batch_executor::traits::{BatchVm, BatchVmFactory, TraceCalls, VmTransactionOutput};

pub(super) struct VmTrackingSnapshots<S: ReadStorage> {
    inner: VmInstance<S, HistoryEnabled>,
}

impl<S: ReadStorage> VmTrackingSnapshots<S> {
    fn assert_not_too_many_snapshots(&self) {
        if let VmInstance::VmFast(vm) = &self.inner {
            let snapshot_count = vm.snapshot_count();
            assert!(
                snapshot_count <= 1,
                "too many snapshots: {snapshot_count:?}"
            );
        } else {
            unreachable!();
        }
    }

    fn assert_exactly_one_snapshot(&self) {
        if let VmInstance::VmFast(vm) = &self.inner {
            assert_eq!(vm.snapshot_count(), 1);
        } else {
            unreachable!();
        }
    }
}

impl<S: ReadStorage> BatchVm for VmTrackingSnapshots<S> {
    fn inspect_transaction(
        &mut self,
        tx: Transaction,
        trace_calls: TraceCalls,
    ) -> VmTransactionOutput {
        self.assert_not_too_many_snapshots();
        let output = self.inner.inspect_transaction(tx, trace_calls);
        self.assert_not_too_many_snapshots();
        output
    }

    fn inspect_transaction_with_optional_compression(
        &mut self,
        tx: Transaction,
        trace_calls: TraceCalls,
    ) -> VmTransactionOutput {
        self.assert_not_too_many_snapshots();
        let output = self
            .inner
            .inspect_transaction_with_optional_compression(tx, trace_calls);
        self.assert_not_too_many_snapshots();
        output
    }

    fn rollback_last_transaction(&mut self) {
        self.assert_exactly_one_snapshot();
        self.inner.rollback_last_transaction();
    }

    fn start_new_l2_block(&mut self, l2_block: L2BlockEnv) {
        self.inner.start_new_l2_block(l2_block);
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        self.assert_not_too_many_snapshots();
        self.inner.finish_batch()
    }
}

#[derive(Debug)]
pub struct VmTrackingSnapshotsFactory;

impl<S: ReadStorage> BatchVmFactory<S> for VmTrackingSnapshotsFactory {
    fn create_vm<'a>(
        &self,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<StorageView<S>>,
    ) -> Box<dyn BatchVm + 'a>
    where
        S: 'a,
    {
        Box::new(VmTrackingSnapshots {
            inner: VmInstance::new(l1_batch_params, system_env, storage),
        })
    }
}
