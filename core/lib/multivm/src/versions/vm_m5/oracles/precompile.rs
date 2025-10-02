use zk_evm_1_3_1::{
    abstractions::{Memory, PrecompileCyclesWitness, PrecompilesProcessor},
    aux_structures::{LogQuery, MemoryQuery, Timestamp},
    precompiles::DefaultPrecompilesProcessor,
};

use super::OracleWithHistory;
use crate::vm_m5::history_recorder::HistoryRecorder;

/// Wrap of DefaultPrecompilesProcessor that store queue
/// of timestamp when precompiles are called to be executed.
///
/// Number of precompiles per block is strictly limited,
/// saving timestamps allows us to check the exact number
/// of log queries, that were used during the tx execution.
#[derive(Debug, Clone)]
pub struct PrecompilesProcessorWithHistory<const B: bool> {
    pub timestamp_history: HistoryRecorder<Vec<Timestamp>>,
    pub default_precompiles_processor: DefaultPrecompilesProcessor<B>,
}

impl<const B: bool> OracleWithHistory for PrecompilesProcessorWithHistory<B> {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.timestamp_history.rollback_to_timestamp(timestamp);
    }

    fn delete_history(&mut self) {
        self.timestamp_history.delete_history();
    }
}

impl<const B: bool> PrecompilesProcessorWithHistory<B> {
    pub fn new() -> Self {
        Self {
            timestamp_history: Default::default(),
            default_precompiles_processor: DefaultPrecompilesProcessor {},
        }
    }
    pub fn get_timestamp_history(&self) -> &Vec<Timestamp> {
        self.timestamp_history.inner()
    }
}

impl<const B: bool> Default for PrecompilesProcessorWithHistory<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const B: bool> PrecompilesProcessor for PrecompilesProcessorWithHistory<B> {
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
