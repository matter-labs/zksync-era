//! Implementations of ZKsync VM executors and executor-related utils.
//!
//! The included implementations are separated from the respective interfaces since they depend
//! on [VM implementations](zksync_multivm), are aware of ZKsync node storage etc.

pub use zksync_multivm::interface::executor as interface;

pub mod batch;
#[cfg(feature = "node_framework")]
pub mod node;
pub mod oneshot;
mod shared;
pub mod storage;
#[cfg(test)]
mod testonly;
pub mod whitelist;
