use std::path::PathBuf;

use ethers::utils::hex;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zksync_basic_types::{web3::Bytes, Address, SLChainId};

use crate::{
    forge_interface::deploy_gateway_ctm::output::DeployGatewayCTMOutput,
    raw::{PatchedConfig, RawConfig},
    traits::{FileConfigWithDefaultName, ZkStackConfig},
    GATEWAY_FILE,
};

/// Config that is only stored for the gateway chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub state_transition_proxy_addr: Address,
    pub state_transition_implementation_addr: Address,
    pub verifier_addr: Address,
    pub validator_timelock_addr: Address,
    pub admin_facet_addr: Address,
    pub mailbox_facet_addr: Address,
    pub executor_facet_addr: Address,
    pub getters_facet_addr: Address,
    pub diamond_init_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub default_upgrade_addr: Address,
    pub multicall3_addr: Address,
    pub relayed_sl_da_validator: Address,
    pub validium_da_validator: Address,
    pub diamond_cut_data: Bytes,
}

impl FileConfigWithDefaultName for GatewayConfig {
    const FILE_NAME: &'static str = GATEWAY_FILE;
}

impl ZkStackConfig for GatewayConfig {}

impl From<DeployGatewayCTMOutput> for GatewayConfig {
    fn from(output: DeployGatewayCTMOutput) -> Self {
        GatewayConfig {
            state_transition_proxy_addr: output
                .gateway_state_transition
                .chain_type_manager_proxy_addr,
            state_transition_implementation_addr: output
                .gateway_state_transition
                .chain_type_manager_implementation_addr,
            verifier_addr: output.gateway_state_transition.verifier_addr,
            admin_facet_addr: output.gateway_state_transition.admin_facet_addr,
            mailbox_facet_addr: output.gateway_state_transition.mailbox_facet_addr,
            executor_facet_addr: output.gateway_state_transition.executor_facet_addr,
            getters_facet_addr: output.gateway_state_transition.getters_facet_addr,
            diamond_init_addr: output.gateway_state_transition.diamond_init_addr,
            genesis_upgrade_addr: output.gateway_state_transition.genesis_upgrade_addr,
            default_upgrade_addr: output.gateway_state_transition.default_upgrade_addr,
            multicall3_addr: output.multicall3_addr,
            diamond_cut_data: hex::decode(output.diamond_cut_data.clone()).unwrap().into(),
            validator_timelock_addr: output.gateway_state_transition.validator_timelock_addr,
            relayed_sl_da_validator: output.relayed_sl_da_validator,
            validium_da_validator: output.validium_da_validator,
        }
    }
}

#[derive(Debug)]
pub struct GatewayChainConfig(RawConfig);

impl GatewayChainConfig {
    pub async fn read(shell: &Shell, path: PathBuf) -> anyhow::Result<Self> {
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
    pub fn empty(shell: &Shell, path: PathBuf) -> Self {
        Self(PatchedConfig::empty(shell, path))
    }

    pub fn init(
        &mut self,
        gateway_config: &GatewayConfig,
        diamond_proxy_addr: Address,
        l2_chain_admin_addr: Address,
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
        self.0
            .insert_yaml("chain_admin_addr", l2_chain_admin_addr)?;
        self.0.insert_yaml("governance_addr", l2_chain_admin_addr)?;
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
