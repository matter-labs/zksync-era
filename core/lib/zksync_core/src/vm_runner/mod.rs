mod storage;

#[cfg(test)]
mod tests;

pub use storage::{BatchData, VmRunnerStorage, VmRunnerStorageLoader};
