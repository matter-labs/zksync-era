pub use self::{
    execution_mode::VmExecutionMode,
    l1_batch_env::L1BatchEnv,
    l2_block::{L2BlockEnv, StoredL2BlockEnv},
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
    /// Part of the environment representing the current L2 block. Can be used to override storage slots
    /// in the system context contract, which are set from `L1BatchEnv.first_l2_block` by default.
    pub current_block: Option<StoredL2BlockEnv>,
}
