//! VM Runner is a framework to build batch processor components, i.e. components that would re-run
//! batches in VM independently from state keeper and handle some output as a result.

#![warn(missing_debug_implementations, missing_docs)]

mod impls;
mod io;
mod output_handler;
mod process;
mod storage;

mod metrics;
#[cfg(test)]
mod tests;

pub use impls::{
    BasicWitnessInputProducer, BasicWitnessInputProducerIo, BasicWitnessInputProducerTasks,
    ProtectiveReadsIo, ProtectiveReadsWriter, ProtectiveReadsWriterTasks,
};
pub use io::VmRunnerIo;
pub use output_handler::{
    ConcurrentOutputHandlerFactory, ConcurrentOutputHandlerFactoryTask, OutputHandlerFactory,
};
pub use process::VmRunner;
pub use storage::{BatchExecuteData, StorageSyncTask, VmRunnerStorage};
