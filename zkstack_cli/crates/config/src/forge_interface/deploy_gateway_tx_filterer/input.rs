use ethers::types::{Address, H256};
use serde::{Deserialize, Serialize};

use crate::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig, traits::ZkStackConfig,
    ContractsConfig,
};

/// Represents the input config for deploying the GatewayTransactionFilterer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayTxFiltererInput {
    pub bridgehub_proxy_addr: Address,
    pub chain_admin: Address,
    pub chain_proxy_admin: Address,
    pub create2_factory_addr: Address,
    pub create2_factory_salt: H256,
}

impl ZkStackConfig for GatewayTxFiltererInput {}

impl GatewayTxFiltererInput {
    pub fn new(
        init_config: &InitialDeploymentConfig,
        contracts_config: &ContractsConfig,
    ) -> anyhow::Result<Self> {
        let bridgehub_proxy_addr = contracts_config.ecosystem_contracts.bridgehub_proxy_addr;

        let chain_admin = contracts_config.l1.chain_admin_addr;

        let chain_proxy_admin = contracts_config.l1.chain_proxy_admin_addr.ok_or_else(|| {
            anyhow::anyhow!("Missing `chain_proxy_admin_addr` in ContractsConfig")
        })?;

        // We provide 0 by default.
        let create2_factory_addr = init_config
            .create2_factory_addr
            .or(Some(contracts_config.create2_factory_addr))
            .unwrap_or_default();
        let create2_factory_salt = init_config.create2_factory_salt;

        Ok(Self {
            bridgehub_proxy_addr,
            chain_admin,
            chain_proxy_admin,
            create2_factory_addr,
            create2_factory_salt,
        })
    }
}
