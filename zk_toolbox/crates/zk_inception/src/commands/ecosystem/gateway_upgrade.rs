use std::{path::PathBuf, str::FromStr};

use anyhow::Context;
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    git,
    hardhat::{build_l1_contracts, build_l2_contracts},
    logger,
    spinner::Spinner,
    Prompt,
};
use config::{
    forge_interface::{
        deploy_ecosystem::{
            input::{DeployErc20Config, Erc20DeploymentConfig, InitialDeploymentConfig},
            output::ERC20Tokens,
        },
        gateway_ecosystem_upgrade::{
            input::GatewayEcosystemUpgradeInput, output::GatewayEcosystemUpgradeOutput,
        },
        script_params::{DEPLOY_ERC20_SCRIPT_PARAMS, GATEWAY_UPGRADE_ECOSYSTEM_PARAMS},
    },
    traits::{
        FileConfigWithDefaultName, ReadConfig, ReadConfigWithBasePath, SaveConfig,
        SaveConfigWithBasePath,
    },
    ContractsConfig, EcosystemConfig, GenesisConfig, WalletsConfig, CONFIGS_PATH,
};
use ethers::abi::Address;
use types::{L1Network, ProverMode};
use xshell::Shell;

use super::{
    args::{
        gateway_upgrade::{GatewayUpgradeArgs, GatewayUpgradeArgsFinal},
        init::{EcosystemArgsFinal, EcosystemInitArgs, EcosystemInitArgsFinal},
    },
    common::deploy_l1,
    setup_observability,
    utils::{build_da_contracts, build_system_contracts, install_yarn_dependencies},
};
use crate::{
    accept_ownership::{accept_admin, accept_owner},
    commands::{
        chain::{self},
        ecosystem::{
            args::gateway_upgrade::GatewayUpgradeStage,
            create_configs::{create_erc20_deployment_config, create_initial_deployments_config},
        },
    },
    messages::{
        msg_ecosystem_initialized, msg_ecosystem_no_found_preexisting_contract,
        msg_initializing_chain, MSG_CHAIN_NOT_INITIALIZED,
        MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_DEPLOYING_ERC20,
        MSG_DEPLOYING_ERC20_SPINNER, MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR,
        MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT, MSG_INITIALIZING_ECOSYSTEM,
        MSG_INTALLING_DEPS_SPINNER,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

pub async fn run(args: GatewayUpgradeArgs, shell: &Shell) -> anyhow::Result<()> {
    println!("Runnig ecosystem gateway upgrade args");

    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;

    let mut final_ecosystem_args = args.fill_values_with_prompt(ecosystem_config.l1_network, true);

    match final_ecosystem_args.ecosystem_upgrade_stage {
        GatewayUpgradeStage::NoGovernancePrepare => {
            no_governance_prepare(&mut final_ecosystem_args, shell, &ecosystem_config).await?;
        }
        GatewayUpgradeStage::GovernanceStage1 => {
            panic!("Stage1 not implemented yet");
        }
        GatewayUpgradeStage::GovernanceStage2 => {
            panic!("Stage2 not implemented yet");
        }
    }

    Ok(())
}

async fn no_governance_prepare(
    init_args: &mut GatewayUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    spinner.finish();

    let forge_args = init_args.forge_args.clone();
    let l1_rpc_url = init_args.l1_rpc_url.clone();

    let new_genesis_config = GenesisConfig::read_with_base_path(shell, CONFIGS_PATH)?;
    let current_contracts_config = ecosystem_config.get_contracts_config()?;
    let initial_deployment_config = ecosystem_config.get_initial_deployment_config()?;
    let wallets_config = ecosystem_config.get_wallets()?;

    let ecosystem_upgrade_config_path =
        GATEWAY_UPGRADE_ECOSYSTEM_PARAMS.input(&ecosystem_config.link_to_code);

    let era_config = ecosystem_config
        .load_chain(Some("era".to_string()))
        .context("No era")?;

    // FIXME: probably gotta force this one
    // assert_eq!(era_config.chain_id, ecosystem_config.era_chain_id);

    let gateway_upgrade_input = GatewayEcosystemUpgradeInput::new(
        &new_genesis_config,
        &current_contracts_config,
        &initial_deployment_config,
        &wallets_config,
        ecosystem_config.era_chain_id,
        // FIXME: provide correct era diamond proxy
        era_config.get_contracts_config()?.l1.diamond_proxy_addr,
        ecosystem_config.prover_version == ProverMode::NoProofs,
    );
    gateway_upgrade_input.save(shell, ecosystem_upgrade_config_path.clone())?;

    let mut forge = Forge::new(&ecosystem_config.path_to_l1_foundry())
        .script(
            &GATEWAY_UPGRADE_ECOSYSTEM_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_slow()
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        ecosystem_config.get_wallets()?.deployer_private_key(),
    )?;

    println!("Preparing the ecosystem for the upgrade!");

    forge.run(shell)?;

    println!("done!");

    let output = GatewayEcosystemUpgradeOutput::read(
        shell,
        GATEWAY_UPGRADE_ECOSYSTEM_PARAMS.output(&ecosystem_config.link_to_code),
    )?;
    output.save_with_base_path(shell, &ecosystem_config.config)?;

    // Each chain will have to update it

    Ok(())
}

// async fn update_chains(
//     ecosystem_config: &EcosystemConfig,
//     output: GatewayEcosystemUpgradeOutput
// ) -> anyhow::Result<()> {
//     for entry in ecosystem_config.list_of_chains() {
//         let chain_config = ecosystem_config.load_chain(Some(entry)).context("unexisting chain")?;

//         let mut contracts_config = chain_config.get_contracts_config()?;

//     }

//     Ok(())
// }
