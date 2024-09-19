use serde::Deserialize;

/// Configuration for the use evm simulator
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct UseEvmSimulator {
    pub use_evm_simulator: bool,
}
