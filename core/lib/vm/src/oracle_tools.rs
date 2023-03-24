use crate::memory::SimpleMemory;
use std::cell::RefCell;

use std::fmt::Debug;
use std::rc::Rc;

use crate::event_sink::InMemoryEventSink;
use crate::oracles::decommitter::DecommitterOracle;
use crate::oracles::precompile::PrecompilesProcessorWithHistory;
use crate::oracles::storage::StorageOracle;
use crate::storage::Storage;
use zk_evm::witness_trace::DummyTracer;

#[derive(Debug)]
pub struct OracleTools<'a, const B: bool> {
    pub storage: StorageOracle<'a>,
    pub memory: SimpleMemory,
    pub event_sink: InMemoryEventSink,
    pub precompiles_processor: PrecompilesProcessorWithHistory<B>,
    pub decommittment_processor: DecommitterOracle<'a, B>,
    pub witness_tracer: DummyTracer,
}

impl<'a> OracleTools<'a, false> {
    pub fn new(storage_view: &'a mut dyn Storage) -> Self {
        let pointer: Rc<RefCell<&'a mut dyn Storage>> = Rc::new(RefCell::new(storage_view));

        Self {
            storage: StorageOracle::new(pointer.clone()),
            memory: SimpleMemory::default(),
            event_sink: InMemoryEventSink::default(),
            precompiles_processor: PrecompilesProcessorWithHistory::default(),
            decommittment_processor: DecommitterOracle::new(pointer.clone()),
            witness_tracer: DummyTracer {},
        }
    }
}
