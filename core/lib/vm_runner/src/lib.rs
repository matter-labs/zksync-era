//! Common components for services that re-execute transactions in zkSync Era

// Linter settings.
#![warn(missing_debug_implementations, missing_docs, bare_trait_objects)]
#![warn(clippy::all, clippy::pedantic)]

mod storage;

pub use storage::{BatchData, VmRunnerStorage, VmRunnerStorageLoader};
