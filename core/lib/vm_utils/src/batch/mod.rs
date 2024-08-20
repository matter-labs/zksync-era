//! Main implementation of batch executor.

pub use self::{executor::MainBatchExecutor, handle::MainBatchExecutorHandle};

mod executor;
mod handle;
mod metrics;
