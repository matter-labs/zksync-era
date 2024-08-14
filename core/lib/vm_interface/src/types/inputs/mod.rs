pub use self::{
    execution_mode::VmExecutionMode,
    l1_batch_env::L1BatchEnv,
    l2_block::{L2BlockEnv, PendingL2BlockEnv},
    system_env::{SystemEnv, TxExecutionMode},
};

mod execution_mode;
mod l1_batch_env;
mod l2_block;
mod system_env;

/// Full environment for oneshot transaction / call execution.
#[derive(Debug)]
pub struct OneshotEnv {
    /// System environment.
    pub system: SystemEnv,
    /// Part of the environment specific to an L1 batch.
    pub l1_batch: L1BatchEnv,
    /// Part of the environment representing a pending L2 block.
    pub pending_block: Option<PendingL2BlockEnv>,
}
