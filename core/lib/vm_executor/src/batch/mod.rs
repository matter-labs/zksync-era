//! Main implementation of ZKsync VM [batch executor](crate::interface::BatchExecutor).
//!
//! This implementation is used by various ZKsync components, like the state keeper and components based on the VM runner.

// Public re-exports to allow crate users not to export the `multivm` crate directly.
pub use zksync_multivm::{dump, shadow};

pub use self::{executor::MainBatchExecutor, factory::MainBatchExecutorFactory};

mod executor;
mod factory;
mod metrics;
