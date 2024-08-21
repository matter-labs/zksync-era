//! Main implementation of batch executor.

pub use self::{executor::MainBatchExecutor, factory::MainBatchExecutorFactory};

mod executor;
mod factory;
mod metrics;
