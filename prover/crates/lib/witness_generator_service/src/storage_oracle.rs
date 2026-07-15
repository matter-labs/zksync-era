use zkevm_test_harness::zk_evm::{
    abstractions::{Storage, StorageAccessRefund},
    aux_structures::{LogQuery, PubdataCost, Timestamp},
    zkevm_opcode_defs::system_params::{STORAGE_AUX_BYTE, TRANSIENT_STORAGE_AUX_BYTE},
};

// Due to traces, we've noticed in the past that storage_refunds and pubdata_costs can be different than actual state_keeper's run.
// Whilst this may not be true today, the storage oracle implementation in witness_generator guards us from such issues in the future.
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
            // storage_refunds as precalculated in state_keeper
            storage_refunds: storage_refunds.into_iter(),
            // pubdata_costs as precalculated in state_keeper
            pubdata_costs: pubdata_costs.into_iter(),
        }
    }
}

impl<T: Storage> Storage for StorageOracle<T> {
    fn get_access_refund(
        &mut self,
        _monotonic_cycle_counter: u32,
        partial_query: &LogQuery,
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

    fn start_new_tx(&mut self, timestamp: Timestamp) {
        self.inn.start_new_tx(timestamp)
    }

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

    fn finish_frame(&mut self, timestamp: Timestamp, panicked: bool) {
        self.inn.finish_frame(timestamp, panicked)
    }

    fn start_frame(&mut self, timestamp: Timestamp) {
        self.inn.start_frame(timestamp)
    }
}
