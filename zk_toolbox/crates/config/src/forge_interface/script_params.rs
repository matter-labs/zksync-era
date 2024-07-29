use std::path::{Path, PathBuf};

use crate::consts::L1_CONTRACTS_FOUNDRY;

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

pub const DEPLOY_ECOSYSTEM_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-deploy-l1.toml",
    output: "script-out/output-deploy-l1.toml",
    script_path: "deploy-scripts/DeployL1.s.sol",
};

pub const DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-deploy-l2-contracts.toml",
    output: "script-out/output-deploy-l2-contracts.toml",
    script_path: "deploy-scripts/DeployL2Contracts.sol",
};

pub const REGISTER_CHAIN_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/register-hyperchain.toml",
    output: "script-out/output-register-hyperchain.toml",
    script_path: "deploy-scripts/RegisterHyperchain.s.sol",
};

pub const DEPLOY_ERC20_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-deploy-erc20.toml",
    output: "script-out/output-deploy-erc20.toml",
    script_path: "deploy-scripts/DeployErc20.s.sol",
};

pub const DEPLOY_PAYMASTER_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-deploy-paymaster.toml",
    output: "script-out/output-deploy-paymaster.toml",
    script_path: "deploy-scripts/DeployPaymaster.s.sol",
};

pub const ACCEPT_GOVERNANCE_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-accept-admin.toml",
    output: "script-out/output-accept-admin.toml",
    script_path: "deploy-scripts/AcceptAdmin.s.sol",
};
