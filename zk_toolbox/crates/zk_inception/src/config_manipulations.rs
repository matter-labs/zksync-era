use common::db::DatabaseConfig;
use config::{
    forge_interface::{
        initialize_bridges::output::InitializeBridgeOutput, paymaster::DeployPaymasterOutput,
        register_chain::output::RegisterChainOutput,
    },
    traits::{ReadConfigWithBasePath, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, GeneralConfig, GenesisConfig, SecretsConfig,
};
use types::ProverMode;
use xshell::Shell;

use crate::defaults::{ROCKS_DB_STATE_KEEPER, ROCKS_DB_TREE};

pub(crate) fn update_genesis(shell: &Shell, config: &ChainConfig) -> anyhow::Result<()> {
    let mut genesis = GenesisConfig::read_with_base_path(shell, &config.configs)?;

    genesis.l2_chain_id = config.chain_id;
    genesis.l1_chain_id = config.l1_network.chain_id();
    genesis.l1_batch_commit_data_generator_mode = Some(config.l1_batch_commit_data_generator_mode);

    genesis.save_with_base_path(shell, &config.configs)?;
    Ok(())
}

pub(crate) fn update_database_secrets(
    shell: &Shell,
    config: &ChainConfig,
    server_db_config: &DatabaseConfig,
    prover_db_config: &DatabaseConfig,
) -> anyhow::Result<()> {
    let mut secrets = SecretsConfig::read_with_base_path(shell, &config.configs)?;
    secrets.database.server_url = server_db_config.full_url();
    secrets.database.prover_url = prover_db_config.full_url();
    secrets.save_with_base_path(shell, &config.configs)?;
    Ok(())
}

pub(crate) fn update_l1_rpc_url_secret(
    shell: &Shell,
    config: &ChainConfig,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let mut secrets = SecretsConfig::read_with_base_path(shell, &config.configs)?;
    secrets.l1.l1_rpc_url = l1_rpc_url;
    secrets.save_with_base_path(shell, &config.configs)?;
    Ok(())
}

pub(crate) fn update_general_config(shell: &Shell, config: &ChainConfig) -> anyhow::Result<()> {
    let mut general = GeneralConfig::read_with_base_path(shell, &config.configs)?;
    general.db.state_keeper_db_path =
        shell.create_dir(config.rocks_db_path.join(ROCKS_DB_STATE_KEEPER))?;
    general.db.merkle_tree.path = shell.create_dir(config.rocks_db_path.join(ROCKS_DB_TREE))?;
    if config.prover_version != ProverMode::NoProofs {
        general.eth.sender.proof_sending_mode = "ONLY_REAL_PROOFS".to_string();
    }
    general.save_with_base_path(shell, &config.configs)?;
    Ok(())
}

pub fn update_l1_contracts(
    shell: &Shell,
    config: &ChainConfig,
    register_chain_output: &RegisterChainOutput,
) -> anyhow::Result<ContractsConfig> {
    let mut contracts_config = ContractsConfig::read_with_base_path(shell, &config.configs)?;
    contracts_config.l1.diamond_proxy_addr = register_chain_output.diamond_proxy_addr;
    contracts_config.l1.governance_addr = register_chain_output.governance_addr;
    contracts_config.save_with_base_path(shell, &config.configs)?;
    Ok(contracts_config)
}

pub fn update_l2_shared_bridge(
    shell: &Shell,
    config: &ChainConfig,
    initialize_bridges_output: &InitializeBridgeOutput,
) -> anyhow::Result<()> {
    let mut contracts_config = ContractsConfig::read_with_base_path(shell, &config.configs)?;
    contracts_config.bridges.shared.l2_address =
        Some(initialize_bridges_output.l2_shared_bridge_proxy);
    contracts_config.bridges.erc20.l2_address =
        Some(initialize_bridges_output.l2_shared_bridge_proxy);
    contracts_config.save_with_base_path(shell, &config.configs)?;
    Ok(())
}

pub fn update_paymaster(
    shell: &Shell,
    config: &ChainConfig,
    paymaster_output: &DeployPaymasterOutput,
) -> anyhow::Result<()> {
    let mut contracts_config = ContractsConfig::read_with_base_path(shell, &config.configs)?;
    contracts_config.l2.testnet_paymaster_addr = paymaster_output.paymaster;
    contracts_config.save_with_base_path(shell, &config.configs)?;
    Ok(())
}
