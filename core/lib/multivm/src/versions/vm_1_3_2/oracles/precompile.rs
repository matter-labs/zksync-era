use zk_evm_1_3_3::{
    abstractions::{Memory, PrecompileCyclesWitness, PrecompilesProcessor},
    aux_structures::{LogQuery, MemoryQuery, Timestamp},
    precompiles::DefaultPrecompilesProcessor,
};

use super::OracleWithHistory;
use crate::vm_1_3_2::history_recorder::{HistoryEnabled, HistoryMode, HistoryRecorder};

/// Wrap of DefaultPrecompilesProcessor that store queue
/// of timestamp when precompiles are called to be executed.
///
/// Number of precompiles per block is strictly limited,
/// saving timestamps allows us to check the exact number
/// of log queries, that were used during the tx execution.
#[derive(Debug, Clone)]
pub struct PrecompilesProcessorWithHistory<const B: bool, H: HistoryMode> {
    pub timestamp_history: HistoryRecorder<Vec<Timestamp>, H>,
    pub default_precompiles_processor: DefaultPrecompilesProcessor<B>,
}

impl<const B: bool, H: HistoryMode> Default for PrecompilesProcessorWithHistory<B, H> {
    fn default() -> Self {
        Self {
            timestamp_history: Default::default(),
            default_precompiles_processor: DefaultPrecompilesProcessor,
        }
    }
}

impl<const B: bool> OracleWithHistory for PrecompilesProcessorWithHistory<B, HistoryEnabled> {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.timestamp_history.rollback_to_timestamp(timestamp);
    }
}

impl<const B: bool, H: HistoryMode> PrecompilesProcessorWithHistory<B, H> {
    pub fn get_timestamp_history(&self) -> &Vec<Timestamp> {
        self.timestamp_history.inner()
    }

    pub fn delete_history(&mut self) {
        self.timestamp_history.delete_history();
    }
}

impl<const B: bool, H: HistoryMode> PrecompilesProcessor for PrecompilesProcessorWithHistory<B, H> {
    fn start_frame(&mut self) {
        self.default_precompiles_processor.start_frame();
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
        self.default_precompiles_processor.execute_precompile(
            monotonic_cycle_counter,
            query,
            memory,
        )
    }
    fn finish_frame(&mut self, _panicked: bool) {
        self.default_precompiles_processor.finish_frame(_panicked);
    }
}
