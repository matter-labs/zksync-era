//! VM Runner is a framework to build batch processor components, i.e. components that would re-run
//! batches in VM independently from state keeper and handle some output as a result.

#![warn(missing_debug_implementations, missing_docs)]

pub mod impls;
mod io;
mod metrics;
pub mod node;
mod output_handler;
mod process;
mod storage;
#[cfg(test)]
mod tests;

pub use self::{
    io::VmRunnerIo,
    output_handler::{
        ConcurrentOutputHandlerFactory, ConcurrentOutputHandlerFactoryTask, L1BatchOutput,
        L2BlockOutput, OutputHandler, OutputHandlerFactory,
    },
    process::VmRunner,
    storage::{BatchExecuteData, StorageSyncTask, VmRunnerStorage},
};
