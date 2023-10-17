use zk_evm_1_3_1::aux_structures::Timestamp;
// We will discard RAM as soon as the execution of a tx ends, so
// it is ok for now to use SimpleMemory
pub use zk_evm_1_3_1::reference_impls::memory::SimpleMemory as RamOracle;
// All the changes to the events in the DB will be applied after the tx is executed,
// so fow now it is fine.
pub use zk_evm_1_3_1::reference_impls::event_sink::InMemoryEventSink as EventSinkOracle;

pub use zk_evm_1_3_1::testing::simple_tracer::NoopTracer;

pub mod decommitter;
pub mod precompile;
pub mod storage;
pub mod tracer;

pub trait OracleWithHistory {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp);
}
