// use circuit_definitions::zk_evm::abstractions::{Storage /*RefundType, RefundAmounts*/};

use zk_evm::{
    abstractions::{/*RefundType, RefundedAmounts, */ Storage, StorageAccessRefund},
    aux_structures::{LogQuery, PubdataCost, Timestamp},
    zkevm_opcode_defs::system_params::{STORAGE_AUX_BYTE, TRANSIENT_STORAGE_AUX_BYTE},
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
        if partial_query.aux_byte == TRANSIENT_STORAGE_AUX_BYTE {
            // Any transient access is warm. Also, no refund needs to be provided as it is already cheap
            StorageAccessRefund::Warm { ergs: 0 }
        } else if partial_query.aux_byte == STORAGE_AUX_BYTE {
            let storage_refunds = self.storage_refunds.next().expect("Missing refund");
            if storage_refunds == 0 {
                StorageAccessRefund::Cold
            } else {
                StorageAccessRefund::Warm {
                    ergs: storage_refunds,
                }
            }
        } else {
            unreachable!()
        }
    }

    fn start_new_tx(&mut self, _timestamp: zk_evm::aux_structures::Timestamp) {
        self.inn.start_new_tx(_timestamp)
    }

    fn execute_partial_query(
        &mut self,
        monotonic_cycle_counter: u32,
        query: LogQuery,
    ) -> (LogQuery, PubdataCost) {
        let (query, wrong_val) = self
            .inn
            .execute_partial_query(monotonic_cycle_counter, query);
        (query, wrong_val)
    }

    fn finish_frame(&mut self, timestamp: Timestamp, panicked: bool) {
        self.inn.finish_frame(timestamp, panicked)
    }

    fn start_frame(&mut self, timestamp: Timestamp) {
        self.inn.start_frame(timestamp)
    }
}
