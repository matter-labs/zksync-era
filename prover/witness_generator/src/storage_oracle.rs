// use circuit_definitions::zk_evm::abstractions::{Storage /*RefundType, RefundAmounts*/};

use zk_evm::{
    abstractions::{/*RefundType, RefundedAmounts, */ Storage, StorageAccessRefund},
    aux_structures::{LogQuery, PubdataCost, Timestamp},
};
// use zkevm_test_harness::zk_evm::abstractions::{RefundType, RefundedAmounts, Storage};

#[derive(Debug)]
pub struct StorageOracle<T> {
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
    // fn estimate_refunds_for_write(
    //     &mut self,
    //     _monotonic_cycle_counter: u32,
    //     _partial_query: &LogQuery,
    // ) -> RefundType {
    //     let pubdata_bytes = self.storage_refunds.next().expect("Missing refund");
    //     RefundType::RepeatedWrite(RefundedAmounts {
    //         pubdata_bytes,
    //         ergs: 0,
    //     })
    // }

    fn get_access_refund(
        &mut self,
        _monotonic_cycle_counter: u32,
        partial_query: &zk_evm::aux_structures::LogQuery,
    ) -> StorageAccessRefund {
        // todo!() -- to figure out what needs to go in here
        // if partial_query.aux_byte == 4 {
        //     // Any transient access is warm. Also, no refund needs to be provided as it is already cheap
        //     StorageAccessRefund::Warm { ergs: 0 }
        // } else {
        StorageAccessRefund::Cold
        // }
    }

    fn start_new_tx(&mut self, _timestamp: zk_evm::aux_structures::Timestamp) {
        // todo!()
    }

    fn execute_partial_query(
        &mut self,
        monotonic_cycle_counter: u32,
        query: LogQuery,
    ) -> (LogQuery, PubdataCost) {
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
