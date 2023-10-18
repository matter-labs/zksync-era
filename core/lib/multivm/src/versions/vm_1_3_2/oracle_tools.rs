use std::fmt::Debug;

use crate::vm_1_3_2::event_sink::InMemoryEventSink;
use crate::vm_1_3_2::history_recorder::HistoryMode;
use crate::vm_1_3_2::memory::SimpleMemory;
use crate::vm_1_3_2::oracles::{
    decommitter::DecommitterOracle, precompile::PrecompilesProcessorWithHistory,
    storage::StorageOracle,
};
use zk_evm_1_3_3::witness_trace::DummyTracer;
use zksync_state::{StoragePtr, WriteStorage};

/// zkEVM requires a bunch of objects implementing given traits to work.
/// For example: Storage, Memory, PrecompilerProcessor etc
/// (you can find all these traites in zk_evm crate -> src/abstractions/mod.rs)
/// For each of these traits, we have a local implementation (for example StorageOracle)
/// that also support additional features (like rollbacks & history).
/// The OracleTools struct, holds all these things together in one place.
#[derive(Debug)]
pub struct OracleTools<S: WriteStorage, const B: bool, H: HistoryMode> {
    pub storage: StorageOracle<S, H>,
    pub memory: SimpleMemory<H>,
    pub event_sink: InMemoryEventSink<H>,
    pub precompiles_processor: PrecompilesProcessorWithHistory<B, H>,
    pub decommittment_processor: DecommitterOracle<S, B, H>,
    pub witness_tracer: DummyTracer,
}

impl<S: WriteStorage, H: HistoryMode> OracleTools<S, false, H> {
    pub fn new(storage_view: StoragePtr<S>) -> Self {
        Self {
            storage: StorageOracle::new(storage_view.clone()),
            memory: SimpleMemory::default(),
            event_sink: InMemoryEventSink::default(),
            precompiles_processor: PrecompilesProcessorWithHistory::default(),
            decommittment_processor: DecommitterOracle::new(storage_view),
            witness_tracer: DummyTracer {},
        }
    }
}
