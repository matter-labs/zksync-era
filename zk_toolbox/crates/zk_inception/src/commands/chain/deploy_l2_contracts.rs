use std::path::Path;

use anyhow::Context;
use common::{
    cmd::Cmd,
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use config::{
    forge_interface::{
        deploy_l2_contracts::{
            input::DeployL2ContractsInput,
            output::{ConsensusRegistryOutput, DefaultL2UpgradeOutput, InitializeBridgeOutput},
        },
        script_params::DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, EcosystemConfig,
};
use xshell::{cmd, Shell};

use crate::{
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_DEPLOYING_L2_CONTRACT_SPINNER,
        MSG_L1_SECRETS_MUST_BE_PRESENTED,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

pub enum Deploy2ContractsOption {
    All,
    Upgrader,
    InitiailizeBridges,
    ConsensusRegistry,
}

pub async fn run(
    args: ForgeScriptArgs,
    shell: &Shell,
    deploy_option: Deploy2ContractsOption,
) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let mut contracts = chain_config.get_contracts_config()?;

    let spinner = Spinner::new(MSG_DEPLOYING_L2_CONTRACT_SPINNER);

    match deploy_option {
        Deploy2ContractsOption::All => {
            deploy_l2_contracts(
                shell,
                &chain_config,
                &ecosystem_config,
                &mut contracts,
                args,
            )
            .await?;
        }
        Deploy2ContractsOption::Upgrader => {
            deploy_upgrader(
                shell,
                &chain_config,
                &ecosystem_config,
                &mut contracts,
                args,
            )
            .await?;
        }
        Deploy2ContractsOption::ConsensusRegistry => {
            deploy_consensus_registry(
                shell,
                &chain_config,
                &ecosystem_config,
                &mut contracts,
                args,
            )
            .await?;
        }
        Deploy2ContractsOption::InitiailizeBridges => {
            initialize_bridges(
                shell,
                &chain_config,
                &ecosystem_config,
                &mut contracts,
                args,
            )
            .await?
        }
    }

    contracts.save_with_base_path(shell, &chain_config.configs)?;
    spinner.finish();

    Ok(())
}

/// Build the L2 contracts, deploy one or all of them with `forge`, then update the config
/// by reading one or all outputs written by the deploy scripts.
async fn build_and_deploy(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    forge_args: ForgeScriptArgs,
    signature: Option<&str>,
    mut update_config: impl FnMut(&Shell, &Path) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    build_l2_contracts(shell, &ecosystem_config.link_to_code)?;
    call_forge(shell, chain_config, ecosystem_config, forge_args, signature).await?;
    update_config(
        shell,
        &DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS.output(&chain_config.link_to_code),
    )?;
    Ok(())
}

pub async fn initialize_bridges(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    let signature = if let Some(true) = chain_config.legacy_bridge {
        Some("runDeployLegacySharedBridge")
    } else {
        Some("runDeploySharedBridge")
    };
    build_and_deploy(
        shell,
        chain_config,
        ecosystem_config,
        forge_args,
        signature,
        |shell, out| {
            contracts_config.set_l2_shared_bridge(&InitializeBridgeOutput::read(shell, out)?)
        },
    )
    .await
}

pub async fn deploy_upgrader(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    build_and_deploy(
        shell,
        chain_config,
        ecosystem_config,
        forge_args,
        Some("runDefaultUpgrader"),
        |shell, out| {
            contracts_config.set_default_l2_upgrade(&DefaultL2UpgradeOutput::read(shell, out)?)
        },
    )
    .await
}

pub async fn deploy_consensus_registry(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    build_and_deploy(
        shell,
        chain_config,
        ecosystem_config,
        forge_args,
        Some("runDeployConsensusRegistry"),
        |shell, out| {
            contracts_config.set_consensus_registry(&ConsensusRegistryOutput::read(shell, out)?)
        },
    )
    .await
}

pub async fn deploy_l2_contracts(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    let signature = if let Some(true) = chain_config.legacy_bridge {
        Some("runWithLegacyBridge")
    } else {
        None
    };
    build_and_deploy(
        shell,
        chain_config,
        ecosystem_config,
        forge_args,
        signature,
        |shell, out| {
            contracts_config.set_l2_shared_bridge(&InitializeBridgeOutput::read(shell, out)?)?;
            contracts_config.set_default_l2_upgrade(&DefaultL2UpgradeOutput::read(shell, out)?)?;
            contracts_config.set_consensus_registry(&ConsensusRegistryOutput::read(shell, out)?)?;
            Ok(())
        },
    )
    .await
}

async fn call_forge(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    forge_args: ForgeScriptArgs,
    signature: Option<&str>,
) -> anyhow::Result<()> {
    let input = DeployL2ContractsInput::new(chain_config, ecosystem_config.era_chain_id)?;
    let foundry_contracts_path = chain_config.path_to_foundry();
    let secrets = chain_config.get_secrets_config()?;
    input.save(
        shell,
        DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS.input(&chain_config.link_to_code),
    )?;

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(
            &DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(
            secrets
                .l1
                .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
                .l1_rpc_url
                .expose_str()
                .to_string(),
        )
        .with_broadcast();

    if let Some(signature) = signature {
        forge = forge.with_signature(signature);
    }

    forge = fill_forge_private_key(
        forge,
        ecosystem_config.get_wallets()?.governor_private_key(),
    )?;

    check_the_balance(&forge).await?;
    forge.run(shell)?;
    Ok(())
}

fn build_l2_contracts(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts"));
    Ok(Cmd::new(cmd!(shell, "yarn l2 build")).run()?)
}
