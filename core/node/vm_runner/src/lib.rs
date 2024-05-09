//! VM Runner is a framework to build batch processor components, i.e. components that would re-run
//! batches in VM independently from state keeper and handle some output as a result.

#![warn(missing_debug_implementations, missing_docs)]

mod io;
mod output_handler;
mod storage;

#[cfg(test)]
mod tests;

pub use io::VmRunnerIo;
pub use output_handler::{
    ConcurrentOutputHandlerFactory, ConcurrentOutputHandlerFactoryTask, OutputHandlerFactory,
};
pub use storage::{BatchExecuteData, VmRunnerStorage};
