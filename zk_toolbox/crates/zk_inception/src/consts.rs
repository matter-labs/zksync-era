use std::path::{Path, PathBuf};

use crate::types::ChainId;

/// Name of the main configuration file
pub(super) const CONFIG_NAME: &str = "ZkStack.yaml";
/// Name of the wallets file
pub(super) const WALLETS_FILE: &str = "wallets.yaml";
/// Name of the general config file
pub(super) const GENERAL_FILE: &str = "general.yaml";
/// Name of the genesis config file
pub(super) const GENESIS_FILE: &str = "genesis.yaml";

pub(super) const ERC20_CONFIGS_FILE: &str = "erc20.yaml";
/// Name of the initial deployments config file
pub(super) const INITIAL_DEPLOYMENT_FILE: &str = "initial_deployments.yaml";
/// Name of the erc20 deployments config file
pub(super) const ERC20_DEPLOYMENT_FILE: &str = "erc20_deployments.yaml";
/// Name of the contracts file
pub(super) const CONTRACTS_FILE: &str = "contracts.yaml";
/// Main repository for the zkSync project
pub(super) const ZKSYNC_ERA_GIT_REPO: &str = "https://github.com/matter-labs/zksync-era";
/// Name of the docker-compose file inside zksync repository
pub(super) const DOCKER_COMPOSE_FILE: &str = "docker-compose.yml";
/// Path to the config file with mnemonic for localhost wallets
pub(super) const CONFIGS_PATH: &str = "etc/env/file_based";
pub(super) const LOCAL_CONFIGS_PATH: &str = "configs/";
pub(super) const LOCAL_DB_PATH: &str = "db/";

/// Path to ecosystem contacts
pub(super) const ECOSYSTEM_PATH: &str = "etc/ecosystem";

/// Path to l1 contracts foundry folder inside zksync-era
pub(super) const L1_CONTRACTS_FOUNDRY: &str = "contracts/l1-contracts-foundry";
/// Path to DeployL1.s.sol script inside zksync-era relative to `L1_CONTRACTS_FOUNDRY`

pub(super) const ERA_CHAIN_ID: ChainId = ChainId(270);

#[derive(PartialEq, Debug, Clone)]
pub struct ForgeScriptParams {
    input: &'static str,
    output: &'static str,
    script_path: &'static str,
}

impl ForgeScriptParams {
    // Path to the input file for forge script
    pub fn input(&self, link_to_code: &Path) -> PathBuf {
        link_to_code.join(L1_CONTRACTS_FOUNDRY).join(self.input)
    }

    // Path to the output file for forge script
    pub fn output(&self, link_to_code: &Path) -> PathBuf {
        link_to_code.join(L1_CONTRACTS_FOUNDRY).join(self.output)
    }

    // Path to the script
    pub fn script(&self) -> PathBuf {
        PathBuf::from(self.script_path)
    }
}

pub const DEPLOY_ECOSYSTEM: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-deploy-l1.toml",
    output: "script-out/output-deploy-l1.toml",
    script_path: "script/DeployL1.s.sol",
};

pub const INITIALIZE_BRIDGES: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-initialize-shared-bridges.toml",
    output: "script-out/output-initialize-shared-bridges.toml",
    script_path: "script/InitializeSharedBridgeOnL2.sol",
};

pub const REGISTER_HYPERCHAIN: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/register-hyperchain.toml",
    output: "script-out/output-register-hyperchain.toml",
    script_path: "script/RegisterHyperchain.s.sol",
};

pub const DEPLOY_ERC20: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-deploy-erc20.toml",
    output: "script-out/output-deploy-erc20.toml",
    script_path: "script/DeployErc20.s.sol",
};

pub const DEPLOY_PAYMASTER: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-deploy-paymaster.toml",
    output: "script-out/output-deploy-paymaster.toml",
    script_path: "script/DeployPaymaster.s.sol",
};

pub const ACCEPT_GOVERNANCE: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-accept-admin.toml",
    output: "script-out/output-accept-admin.toml",
    script_path: "script/AcceptAdmin.s.sol",
};
