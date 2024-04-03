use zk_evm::aux_structures::{LogQuery, Timestamp};
use zkevm_test_harness::zk_evm::abstractions::{RefundType, RefundedAmounts, Storage};

#[derive(Debug)]
pub struct StorageOracle<T> {
    inn: T,
    storage_refunds: std::vec::IntoIter<u32>,
    pubdata_costs: std::vec::IntoIter<i32>,
}

impl<T> StorageOracle<T> {
    pub fn new(inn: T, storage_refunds: Vec<u32>, pubdata_costs: Vec<i32>) -> Self {
        Self {
            inn,
            storage_refunds: storage_refunds.into_iter(),
            pubdata_costs: pubdata_costs.into_iter(),
        }
    }
}

impl<T: Storage> Storage for StorageOracle<T> {
    fn execute_partial_query(
        &mut self,
        monotonic_cycle_counter: u32,
        query: LogQuery,
    ) -> (LogQuery, PubdataCost) {
        let (query, _) = self
            .inn
            .execute_partial_query(monotonic_cycle_counter, query);

        let pubdata_cost = self.pubdata_costs.next().expect("Missing pubdata cost");

        (query, PubdataCost(pubdata_cost))
    }

    fn get_access_refund(
        &mut self, // to avoid any hacks inside, like prefetch
        _monotonic_cycle_counter: u32,
        partial_query: &LogQuery,
    ) -> StorageAccessRefund {
        // While the `zk_evm` provides an enum of StorageAccessRefund::Cold and StorageAccessRefund::Warm { refund },
        // in reality the only important number is the refund amount, i.e. StorageAccessRefund::Cold and
        // StorageAccessRefund::Warm { refund: 0 } are the same.
        let refund = self.storage_refunds.next().expect("Missing refund");
        StorageAccessRefund::Warm({ refund })
    }

    fn finish_frame(&mut self, timestamp: Timestamp, panicked: bool) {
        self.inn.finish_frame(timestamp, panicked)
    }

    fn start_frame(&mut self, timestamp: Timestamp) {
        self.inn.start_frame(timestamp)
    }

    fn start_new_tx(&mut self, timestamp: Timestamp) {
        self.inn.start_new_tx(timestamp)
    }
}
