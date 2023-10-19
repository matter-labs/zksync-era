use vm::{HistoryDisabled, StorageOracle as VmStorageOracle};
use zksync_state::{ReadStorage, StoragePtr, StorageView};
use zksync_types::zkevm_test_harness::zk_evm::abstractions::{
    RefundType, RefundedAmounts, Storage,
};
use zksync_types::{LogQuery, Timestamp};

#[derive(Debug)]
pub(super) struct StorageOracle {
    inner: VmStorageOracle<StorageView<Box<dyn ReadStorage>>, HistoryDisabled>,
    storage_refunds: Vec<u32>,
    storage_refunds_pointer: usize,
}

impl StorageOracle {
    pub fn new(
        storage_view: StoragePtr<StorageView<Box<dyn ReadStorage>>>,
        storage_refunds: Vec<u32>,
    ) -> Self {
        Self {
            inner: VmStorageOracle::new(storage_view),
            storage_refunds,
            storage_refunds_pointer: 0,
        }
    }
}

impl Storage for StorageOracle {
    fn execute_partial_query(&mut self, monotonic_cycle_counter: u32, query: LogQuery) -> LogQuery {
        self.inner
            .execute_partial_query(monotonic_cycle_counter, query)
    }

    fn estimate_refunds_for_write(
        &mut self,
        _monotonic_cycle_counter: u32,
        _partial_query: &LogQuery,
    ) -> RefundType {
        let refund = RefundType::RepeatedWrite(RefundedAmounts {
            ergs: 0,
            pubdata_bytes: self.storage_refunds[self.storage_refunds_pointer],
        });
        self.storage_refunds_pointer += 1;

        refund
    }

    fn start_frame(&mut self, timestamp: Timestamp) {
        self.inner.start_frame(timestamp)
    }

    fn finish_frame(&mut self, timestamp: Timestamp, panicked: bool) {
        self.inner.finish_frame(timestamp, panicked)
    }
}
