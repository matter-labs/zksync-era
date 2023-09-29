use crate::memory::SimpleMemory;
use std::cell::RefCell;

use std::fmt::Debug;
use std::rc::Rc;

use crate::event_sink::InMemoryEventSink;
use crate::history_recorder::HistoryMode;
use crate::oracles::{
    decommitter::DecommitterOracle, precompile::PrecompilesProcessorWithHistory,
    storage::StorageOracle,
};
use crate::storage::Storage;
use zk_evm::witness_trace::DummyTracer;

/// zkEVM requires a bunch of objects implementing given traits to work.
/// For example: Storage, Memory, PrecompilerProcessor etc
/// (you can find all these traites in zk_evm crate -> src/abstractions/mod.rs)
/// For each of these traits, we have a local implementation (for example StorageOracle)
/// that also support additional features (like rollbacks & history).
/// The OracleTools struct, holds all these things together in one place.
#[derive(Debug)]
pub struct OracleTools<'a, const B: bool, H: HistoryMode> {
    pub storage: StorageOracle<'a, H>,
    pub memory: SimpleMemory<H>,
    pub event_sink: InMemoryEventSink<H>,
    pub precompiles_processor: PrecompilesProcessorWithHistory<B, H>,
    pub decommittment_processor: DecommitterOracle<'a, B, H>,
    pub witness_tracer: DummyTracer,
}

impl<'a, H: HistoryMode> OracleTools<'a, false, H> {
    pub fn new(storage_view: &'a mut dyn Storage, _: H) -> Self {
        let pointer: Rc<RefCell<&'a mut dyn Storage>> = Rc::new(RefCell::new(storage_view));

        Self {
            storage: StorageOracle::new(pointer.clone()),
            memory: Default::default(),
            event_sink: Default::default(),
            precompiles_processor: Default::default(),
            decommittment_processor: DecommitterOracle::new(pointer.clone()),
            witness_tracer: DummyTracer {},
        }
    }
}
