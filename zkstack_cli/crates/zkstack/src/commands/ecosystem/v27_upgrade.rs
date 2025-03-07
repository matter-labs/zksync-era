use anyhow::Context;
use serde::Deserialize;
use xshell::Shell;
use zkstack_cli_common::{forge::Forge, git, spinner::Spinner};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::input::GenesisInput,
        script_params::V27_UPGRADE_ECOSYSTEM_PARAMS,
        v27_ecosystem_upgrade::{
            input::V27EcosystemUpgradeInput, output::V27EcosystemUpgradeOutput,
        },
    },
    raw::RawConfig,
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    EcosystemConfig, GENESIS_FILE,
};
use zkstack_cli_types::ProverMode;

use super::args::v27_upgrade::{V27UpgradeArgs, V27UpgradeArgsFinal};
use crate::{
    commands::ecosystem::args::v27_upgrade::V27UpgradeStage,
    messages::MSG_INTALLING_DEPS_SPINNER,
    utils::forge::{fill_forge_private_key, WalletOwner},
};

pub async fn run(args: V27UpgradeArgs, shell: &Shell) -> anyhow::Result<()> {
    println!("Running ecosystem v27 upgrade args");

    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;

    let mut final_ecosystem_args = args.fill_values_with_prompt(ecosystem_config.l1_network, true);

    match final_ecosystem_args.ecosystem_upgrade_stage {
        V27UpgradeStage::NoGovernancePrepare => {
            no_governance_prepare(&mut final_ecosystem_args, shell, &ecosystem_config).await?;
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct BroadcastFile {
    pub transactions: Vec<BroadcastFileTransactions>,
}
#[derive(Debug, Deserialize)]
struct BroadcastFileTransactions {
    pub hash: String,
}

async fn no_governance_prepare(
    init_args: &mut V27UpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    spinner.finish();

    let forge_args = init_args.forge_args.clone();
    let l1_rpc_url = init_args.l1_rpc_url.clone();

    let genesis_config_path = ecosystem_config
        .get_default_configs_path()
        .join(GENESIS_FILE);
    let default_genesis_config = RawConfig::read(shell, genesis_config_path).await?;
    let default_genesis_input = GenesisInput::new(&default_genesis_config)?;
    let current_contracts_config = ecosystem_config.get_contracts_config()?;
    let initial_deployment_config = ecosystem_config.get_initial_deployment_config()?;

    let ecosystem_upgrade_config_path =
        V27_UPGRADE_ECOSYSTEM_PARAMS.input(&ecosystem_config.link_to_code);

    let era_config = ecosystem_config
        .load_chain(Some("era".to_string()))
        .context("No era")?;

    // FIXME: we will have to force this in production environment
    // assert_eq!(era_config.chain_id, ecosystem_config.era_chain_id);

    let gateway_upgrade_input = V27EcosystemUpgradeInput::new(
        &default_genesis_input,
        &current_contracts_config,
        &initial_deployment_config,
        ecosystem_config.era_chain_id,
        era_config.get_contracts_config()?.l1.diamond_proxy_addr,
        ecosystem_config.prover_version == ProverMode::NoProofs,
    );
    gateway_upgrade_input.save(shell, ecosystem_upgrade_config_path.clone())?;

    let mut forge = Forge::new(&ecosystem_config.path_to_l1_foundry())
        .script(&V27_UPGRADE_ECOSYSTEM_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_slow()
        .with_gas_limit(1_000_000_000_000)
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        ecosystem_config.get_wallets()?.deployer.as_ref(),
        WalletOwner::Deployer,
    )?;

    println!("Preparing the ecosystem for the upgrade!");

    forge.run(shell)?;

    println!("done!");

    let l1_chain_id = era_config.l1_network.chain_id();

    let broadcast_file: BroadcastFile = {
        let file_content = std::fs::read_to_string(
            ecosystem_config
                .link_to_code
                .join("contracts/l1-contracts")
                .join(format!(
                    "broadcast/EcosystemUpgrade.s.sol/{}/run-latest.json",
                    l1_chain_id
                )),
        )
        .context("Failed to read broadcast file")?;
        serde_json::from_str(&file_content).context("Failed to parse broadcast file")?
    };

    let mut output = V27EcosystemUpgradeOutput::read(
        shell,
        V27_UPGRADE_ECOSYSTEM_PARAMS.output(&ecosystem_config.link_to_code),
    )?;

    // Add all the transaction hashes.
    for tx in broadcast_file.transactions {
        output.transactions.push(tx.hash);
    }

    output.save_with_base_path(shell, &ecosystem_config.config)?;

    Ok(())
}
