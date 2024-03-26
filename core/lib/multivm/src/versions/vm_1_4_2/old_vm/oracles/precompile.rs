use std::convert::TryFrom;

use zk_evm_1_4_1::{
    abstractions::{Memory, PrecompileCyclesWitness, PrecompilesProcessor},
    aux_structures::{LogQuery, MemoryQuery, Timestamp},
    zk_evm_abstractions::precompiles::{ecrecover, keccak256, sha256, PrecompileAddress},
};

use super::OracleWithHistory;
use crate::vm_latest::old_vm::history_recorder::{HistoryEnabled, HistoryMode, HistoryRecorder};

/// Wrap of DefaultPrecompilesProcessor that store queue
/// of timestamp when precompiles are called to be executed.
/// Number of precompiles per block is strictly limited,
/// saving timestamps allows us to check the exact number
/// of log queries, that were used during the tx execution.
#[derive(Debug, Clone)]
pub struct PrecompilesProcessorWithHistory<H: HistoryMode> {
    pub timestamp_history: HistoryRecorder<Vec<Timestamp>, H>,
    pub precompile_cycles_history: HistoryRecorder<Vec<(PrecompileAddress, usize)>, H>,
}

impl<H: HistoryMode> Default for PrecompilesProcessorWithHistory<H> {
    fn default() -> Self {
        Self {
            timestamp_history: Default::default(),
            precompile_cycles_history: Default::default(),
        }
    }
}

impl OracleWithHistory for PrecompilesProcessorWithHistory<HistoryEnabled> {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.timestamp_history.rollback_to_timestamp(timestamp);
        self.precompile_cycles_history
            .rollback_to_timestamp(timestamp);
    }
}

impl<H: HistoryMode> PrecompilesProcessorWithHistory<H> {
    pub fn get_timestamp_history(&self) -> &Vec<Timestamp> {
        self.timestamp_history.inner()
    }

    pub fn delete_history(&mut self) {
        self.timestamp_history.delete_history();
        self.precompile_cycles_history.delete_history();
    }
}

impl<H: HistoryMode> PrecompilesProcessor for PrecompilesProcessorWithHistory<H> {
    fn start_frame(&mut self) {
        // there are no precompiles to rollback, do nothing
    }

    fn execute_precompile<M: Memory>(
        &mut self,
        monotonic_cycle_counter: u32,
        query: LogQuery,
        memory: &mut M,
    ) -> Option<(Vec<MemoryQuery>, Vec<MemoryQuery>, PrecompileCyclesWitness)> {
        // In the next line we same `query.timestamp` as both
        // an operation in the history of precompiles processor and
        // the time when this operation occurred.
        // While slightly weird, it is done for consistency with other oracles
        // where operations and timestamp have different types.
        self.timestamp_history
            .push(query.timestamp, query.timestamp);

        let address_low = u16::from_le_bytes([query.address.0[19], query.address.0[18]]);
        if let Ok(precompile_address) = PrecompileAddress::try_from(address_low) {
            let rounds = match precompile_address {
                PrecompileAddress::Keccak256 => {
                    // pure function call, non-revertable
                    keccak256::keccak256_rounds_function::<M, false>(
                        monotonic_cycle_counter,
                        query,
                        memory,
                    )
                    .0
                }
                PrecompileAddress::SHA256 => {
                    // pure function call, non-revertable
                    sha256::sha256_rounds_function::<M, false>(
                        monotonic_cycle_counter,
                        query,
                        memory,
                    )
                    .0
                }
                PrecompileAddress::Ecrecover => {
                    // pure function call, non-revertable
                    ecrecover::ecrecover_function::<M, false>(
                        monotonic_cycle_counter,
                        query,
                        memory,
                    )
                    .0
                }
            };

            self.precompile_cycles_history
                .push((precompile_address, rounds), query.timestamp);
        };

        None
    }

    fn finish_frame(&mut self, _panicked: bool) {
        // there are no revertible precompile yes, so we are ok
    }
}
