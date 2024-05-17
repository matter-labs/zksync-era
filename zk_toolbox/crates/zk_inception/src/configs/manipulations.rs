use std::path::Path;

use xshell::Shell;

use crate::{
    configs::{
        contracts::ContractsConfig,
        forge_interface::{
            initialize_bridges::output::InitializeBridgeOutput, paymaster::DeployPaymasterOutput,
            register_hyperchain::output::RegisterHyperchainOutput,
        },
        hyperchain::HyperchainConfig,
        DatabasesConfig, EcosystemConfig, GeneralConfig, GenesisConfig, ReadConfig, SaveConfig,
    },
    consts::{CONFIGS_PATH, CONTRACTS_FILE, GENERAL_FILE, GENESIS_FILE, WALLETS_FILE},
    defaults::{ROCKS_DB_STATE_KEEPER, ROCKS_DB_TREE},
    types::ProverMode,
};

pub(crate) fn copy_configs(
    shell: &Shell,
    link_to_code: &Path,
    hyperchain_config_path: &Path,
) -> anyhow::Result<()> {
    let original_configs = link_to_code.join(CONFIGS_PATH);
    for file in shell.read_dir(original_configs)? {
        if let Some(name) = file.file_name() {
            // Do not copy wallets file
            if name != WALLETS_FILE {
                shell.copy_file(file, hyperchain_config_path)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn update_genesis(shell: &Shell, config: &HyperchainConfig) -> anyhow::Result<()> {
    let path = config.configs.join(GENESIS_FILE);
    let mut genesis = GenesisConfig::read(&path)?;

    genesis.l2_chain_id = config.chain_id;
    genesis.l1_chain_id = config.l1_network.chain_id();
    genesis.l1_batch_commit_data_generator_mode = Some(config.l1_batch_commit_data_generator_mode);

    genesis.save(shell, &path)?;
    Ok(())
}

pub(crate) fn update_general_config(
    shell: &Shell,
    config: &HyperchainConfig,
    db_config: &DatabasesConfig,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let path = config.configs.join(GENERAL_FILE);
    let mut general = GeneralConfig::read(&path)?;
    general.postgres.server_url = db_config.server.full_url();
    general.postgres.prover_url = db_config.prover.full_url();
    general.db.state_keeper_db_path =
        shell.create_dir(config.rocks_db_path.join(ROCKS_DB_STATE_KEEPER))?;
    general.db.merkle_tree.path = shell.create_dir(config.rocks_db_path.join(ROCKS_DB_TREE))?;
    if config.prover_version != ProverMode::NoProofs {
        general.eth.sender.proof_sending_mode = "ONLY_REAL_PROOFS".to_string();
    }
    general
        .eth
        .web3_url
        .clone_from(&ecosystem_config.l1_rpc_url);
    general.save(shell, path)?;
    Ok(())
}

pub fn update_diamond_proxy(
    shell: &Shell,
    config: &HyperchainConfig,
    register_hyperchain_output: &RegisterHyperchainOutput,
) -> anyhow::Result<()> {
    let contracts_config_path = config.configs.join(CONTRACTS_FILE);
    let mut contracts_config = ContractsConfig::read(&contracts_config_path)?;
    contracts_config.l1.diamond_proxy_addr = register_hyperchain_output.diamond_proxy_addr;
    contracts_config.save(shell, &contracts_config_path)?;
    Ok(())
}

pub fn update_l2_shared_bridge(
    shell: &Shell,
    config: &HyperchainConfig,
    initialize_bridges_output: &InitializeBridgeOutput,
) -> anyhow::Result<()> {
    let contracts_config_path = config.configs.join(CONTRACTS_FILE);
    let mut contracts_config = ContractsConfig::read(&contracts_config_path)?;
    contracts_config.bridges.shared.l2_address =
        Some(initialize_bridges_output.l2_shared_bridge_proxy);
    contracts_config.save(shell, &contracts_config_path)?;
    Ok(())
}

pub fn update_paymaster(
    shell: &Shell,
    config: &HyperchainConfig,
    paymaster_output: &DeployPaymasterOutput,
) -> anyhow::Result<()> {
    let contracts_config_path = config.configs.join(CONTRACTS_FILE);
    let mut contracts_config = ContractsConfig::read(&contracts_config_path)?;
    contracts_config.l2.testnet_paymaster_addr = paymaster_output.paymaster;
    contracts_config.save(shell, &contracts_config_path)?;
    Ok(())
}
