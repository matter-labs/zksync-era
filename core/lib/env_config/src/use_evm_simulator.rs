use zksync_config::configs::use_evm_simulator::UseEvmSimulator;

use crate::{envy_load, FromEnv};

impl FromEnv for UseEvmSimulator {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("use_evm_simulator", "USE_EVM_SIMULATOR_")
    }
}
