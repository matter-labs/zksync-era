use std::fmt::Debug;

use zk_evm_1_3_1::witness_trace::DummyTracer;

use crate::vm_m6::{
    event_sink::InMemoryEventSink,
    history_recorder::HistoryMode,
    memory::SimpleMemory,
    oracles::{
        decommitter::DecommitterOracle, precompile::PrecompilesProcessorWithHistory,
        storage::StorageOracle,
    },
    storage::{Storage, StoragePtr},
};

/// zkEVM requires a bunch of objects implementing given traits to work.
///
/// For example: Storage, Memory, PrecompilerProcessor etc
/// (you can find all these traits in zk_evm crate -> src/abstractions/mod.rs)
/// For each of these traits, we have a local implementation (for example StorageOracle)
/// that also support additional features (like rollbacks & history).
/// The OracleTools struct, holds all these things together in one place.
#[derive(Debug)]
pub struct OracleTools<const B: bool, S: Storage, H: HistoryMode> {
    pub storage: StorageOracle<S, H>,
    pub memory: SimpleMemory<H>,
    pub event_sink: InMemoryEventSink<H>,
    pub precompiles_processor: PrecompilesProcessorWithHistory<B, H>,
    pub decommittment_processor: DecommitterOracle<B, S, H>,
    pub witness_tracer: DummyTracer,
}

impl<S: Storage, H: HistoryMode> OracleTools<false, S, H> {
    pub fn new(storage_view: StoragePtr<S>, _: H) -> Self {
        Self {
            storage: StorageOracle::new(storage_view.clone()),
            memory: Default::default(),
            event_sink: Default::default(),
            precompiles_processor: Default::default(),
            decommittment_processor: DecommitterOracle::new(storage_view),
            witness_tracer: DummyTracer {},
        }
    }
}
