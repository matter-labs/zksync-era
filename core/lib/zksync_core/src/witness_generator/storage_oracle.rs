use zksync_types::zkevm_test_harness::zk_evm::abstractions::{
    RefundType, RefundedAmounts, Storage,
};
use zksync_types::{LogQuery, Timestamp};

#[derive(Debug)]
pub(super) struct StorageOracle<T> {
    inn: T,
    storage_refunds: std::vec::IntoIter<u32>,
}

impl<T> StorageOracle<T> {
    pub fn new(inn: T, storage_refunds: Vec<u32>) -> Self {
        Self {
            inn,
            storage_refunds: storage_refunds.into_iter(),
        }
    }
}

impl<T: Storage> Storage for StorageOracle<T> {
    fn estimate_refunds_for_write(
        &mut self,
        _monotonic_cycle_counter: u32,
        _partial_query: &LogQuery,
    ) -> RefundType {
        let pubdata_bytes = self.storage_refunds.next().expect("Missing refund");
        RefundType::RepeatedWrite(RefundedAmounts {
            pubdata_bytes,
            ergs: 0,
        })
    }

    fn execute_partial_query(&mut self, monotonic_cycle_counter: u32, query: LogQuery) -> LogQuery {
        self.inn
            .execute_partial_query(monotonic_cycle_counter, query)
    }

    fn finish_frame(&mut self, timestamp: Timestamp, panicked: bool) {
        self.inn.finish_frame(timestamp, panicked)
    }

    fn start_frame(&mut self, timestamp: Timestamp) {
        self.inn.start_frame(timestamp)
    }
}
