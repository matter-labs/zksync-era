use serde::Deserialize;

/// Configure whether to enable the EVM simulator on the stack.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct UseEvmSimulator {
    pub use_evm_simulator: bool,
}
