use std::path::Path;

use ethers::utils::hex;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zksync_basic_types::{web3::Bytes, Address, SLChainId};

use crate::{
    forge_interface::gateway_vote_preparation::output::DeployGatewayCTMOutput,
    raw::{PatchedConfig, RawConfig},
    traits::{FileConfigWithDefaultName, ZkStackConfigTrait},
    GATEWAY_FILE,
};

/// Config that is only stored for the gateway chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub state_transition_proxy_addr: Address,
    pub validator_timelock_addr: Address,
    pub multicall3_addr: Address,
    pub relayed_sl_da_validator: Address,
    pub rollup_da_manager: Address,
    pub validium_da_validator: Address,
    pub diamond_cut_data: Bytes,
}

impl FileConfigWithDefaultName for GatewayConfig {
    const FILE_NAME: &'static str = GATEWAY_FILE;
}

impl ZkStackConfigTrait for GatewayConfig {}

impl From<DeployGatewayCTMOutput> for GatewayConfig {
    fn from(output: DeployGatewayCTMOutput) -> Self {
        GatewayConfig {
            state_transition_proxy_addr: output
                .gateway_state_transition
                .chain_type_manager_proxy_addr,
            multicall3_addr: output.multicall3_addr,
            diamond_cut_data: hex::decode(output.diamond_cut_data.clone()).unwrap().into(),
            validator_timelock_addr: output.gateway_state_transition.validator_timelock_addr,
            relayed_sl_da_validator: output.relayed_sl_da_validator,
            validium_da_validator: output.validium_da_validator,
            rollup_da_manager: output.gateway_state_transition.rollup_da_manager_addr,
        }
    }
}

#[derive(Debug)]
pub struct GatewayChainConfig(RawConfig);

impl GatewayChainConfig {
    pub async fn read(shell: &Shell, path: &Path) -> anyhow::Result<Self> {
        RawConfig::read(shell, path).await.map(Self)
    }

    pub fn gateway_chain_id(&self) -> anyhow::Result<SLChainId> {
        self.0.get("gateway_chain_id")
    }

    pub fn chain_admin_addr(&self) -> anyhow::Result<Address> {
        self.0.get("chain_admin_addr")
    }

    pub fn patched(self) -> GatewayChainConfigPatch {
        GatewayChainConfigPatch(self.0.patched())
    }
}

pub struct GatewayChainConfigPatch(PatchedConfig);

impl GatewayChainConfigPatch {
    pub fn empty(shell: &Shell, path: &Path) -> Self {
        Self(PatchedConfig::empty(shell, path))
    }

    pub fn init(
        &mut self,
        gateway_config: &GatewayConfig,
        diamond_proxy_addr: Address,
        gateway_chain_id: SLChainId,
    ) -> anyhow::Result<()> {
        self.0.insert_yaml(
            "state_transition_proxy_addr",
            gateway_config.state_transition_proxy_addr,
        )?;
        self.0.insert_yaml(
            "validator_timelock_addr",
            gateway_config.validator_timelock_addr,
        )?;
        self.0
            .insert_yaml("multicall3_addr", gateway_config.multicall3_addr)?;
        self.0
            .insert_yaml("diamond_proxy_addr", diamond_proxy_addr)?;
        self.0.insert_yaml("gateway_chain_id", gateway_chain_id)?;
        Ok(())
    }

    pub fn set_gateway_chain_id(&mut self, chain_id: SLChainId) -> anyhow::Result<()> {
        self.0.insert("gateway_chain_id", chain_id.0)
    }

    pub async fn save(self) -> anyhow::Result<()> {
        self.0.save().await
    }
}
