use std::path::{Path, PathBuf};

#[derive(PartialEq, Debug, Clone)]
pub struct ForgeScriptParams {
    input: &'static str,
    output: &'static str,
    script_path: &'static str,
}

impl ForgeScriptParams {
    // Path to the input file for forge script
    pub fn input(&self, path_to_l1_foundry: &Path) -> PathBuf {
        path_to_l1_foundry.join(self.input)
    }

    // Path to the output file for forge script
    pub fn output(&self, path_to_l1_foundry: &Path) -> PathBuf {
        path_to_l1_foundry.join(self.output)
    }

    // Path to the script
    pub fn script(&self) -> PathBuf {
        PathBuf::from(self.script_path)
    }
}

pub const DEPLOY_CTM_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-deploy-ctm.toml",
    output: "script-out/output-deploy-ctm.toml",
    script_path: "deploy-scripts/DeployCTM.s.sol",
};

pub const DEPLOY_ECOSYSTEM_CORE_CONTRACTS_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-deploy-l1.toml",
    output: "script-out/output-deploy-l1.toml",
    script_path: "deploy-scripts/DeployL1CoreContracts.s.sol",
};

pub const REGISTER_CTM_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-register-ctm-l1.toml",
    output: "script-out/register-ctm-l1.toml",
    script_path: "deploy-scripts/RegisterCTM.s.sol",
};

pub const DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/config-deploy-l2-contracts.toml",
    output: "script-out/output-deploy-l2-contracts.toml",
    script_path: "deploy-scripts/DeployL2Contracts.sol",
};

pub const REGISTER_CHAIN_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/register-zk-chain.toml",
    output: "script-out/output-register-zk-chain.toml",
    script_path: "deploy-scripts/RegisterZKChain.s.sol",
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
    input: "script-config/config-admin-functions.toml",
    output: "script-out/output-admin-functions.toml",
    script_path: "deploy-scripts/AdminFunctions.s.sol",
};

pub const SETUP_LEGACY_BRIDGE: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/setup-legacy-bridge.toml",
    output: "script-out/setup-legacy-bridge.toml",
    script_path: "deploy-scripts/dev/SetupLegacyBridge.s.sol",
};

pub const ENABLE_EVM_EMULATOR_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/enable-evm-emulator.toml",
    output: "script-out/output-enable-evm-emulator.toml",
    script_path: "deploy-scripts/EnableEvmEmulator.s.sol",
};

pub const GATEWAY_UTILS_SCRIPT_PATH: &str = "deploy-scripts/gateway/GatewayUtils.s.sol";

pub const DEPLOY_GATEWAY_TX_FILTERER: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/deploy-gateway-tx-filterer.toml",
    output: "script-out/deploy-gateway-tx-filterer.toml",
    script_path: "deploy-scripts/gateway/DeployGatewayTransactionFilterer.s.sol",
};

pub const GATEWAY_PREPARATION: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/gateway-preparation.toml",
    output: "script-out/gateway-preparation.toml",
    script_path: "deploy-scripts/gateway/GatewayPreparation.s.sol",
};

pub const GATEWAY_VOTE_PREPARATION: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/gateway-vote-preparation.toml",
    output: "script-out/gateway-vote-preparation.toml",
    script_path: "deploy-scripts/gateway/GatewayVotePreparation.s.sol",
};

pub const GATEWAY_GOVERNANCE_TX_PATH1: &str =
    "contracts/l1-contracts/script-out/gateway-deploy-governance-txs-1.json";

pub const V29_UPGRADE_ECOSYSTEM_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/v29-upgrade-ecosystem.toml",
    output: "script-out/v29-upgrade-ecosystem.toml",
    script_path: "deploy-scripts/upgrade/EcosystemUpgrade_v29.s.sol",
};

pub const V29_UPGRADE_CHAIN_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/v29-upgrade-chain.toml",
    output: "script-out/v29-upgrade-chain.toml",
    script_path: "deploy-scripts/upgrade/ChainUpgrade_v29.s.sol",
};

pub const ZK_OS_V28_1_UPGRADE_ECOSYSTEM_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/zk-os-v28-1-upgrade-ecosystem.toml",
    output: "script-out/zk-os-v28-1-upgrade-ecosystem.toml",
    script_path: "deploy-scripts/upgrade/EcosystemUpgrade_v28_1_zk_os.s.sol",
};

pub const ERA_V28_1_UPGRADE_ECOSYSTEM_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/era-v28-1-upgrade-ecosystem.toml",
    output: "script-out/era-v28-1-upgrade-ecosystem.toml",
    script_path: "deploy-scripts/upgrade/EcosystemUpgrade_v28_1.s.sol",
};

pub const ZK_OS_V28_1_UPGRADE_CHAIN_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/zk-os-v28-1-upgrade-chain.toml",
    output: "script-out/zk-os-v28-1-upgrade-chain.toml",
    script_path: "deploy-scripts/upgrade/ChainUpgrade_v28_1_zk_os.s.sol",
};

pub const FINALIZE_UPGRADE_SCRIPT_PARAMS: ForgeScriptParams = ForgeScriptParams {
    input: "script-config/gateway-finalize-upgrade.toml",
    output: "script-out/gateway-finalize-upgrade.toml",
    script_path: "deploy-scripts/upgrade/FinalizeUpgrade.s.sol",
};
