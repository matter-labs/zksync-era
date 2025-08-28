use std::path::Path;

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    contracts::build_l2_contracts,
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_l2_contracts::{
            input::DeployL2ContractsInput,
            output::{
                ConsensusRegistryOutput, DefaultL2UpgradeOutput, Multicall3Output,
                TimestampAsserterOutput,
            },
        },
        script_params::DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, EcosystemConfig, ZkStackConfig,
};

use crate::{
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_DEPLOYING_L2_CONTRACT_SPINNER},
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

pub enum Deploy2ContractsOption {
    All,
    Upgrader,
    ConsensusRegistry,
    Multicall3,
    TimestampAsserter,
    L2DAValidator,
}

pub async fn run(
    args: ForgeScriptArgs,
    shell: &Shell,
    deploy_option: Deploy2ContractsOption,
) -> anyhow::Result<()> {
    // todo we actually need only chain config here
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
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
                true,
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
        Deploy2ContractsOption::Multicall3 => {
            deploy_multicall3(
                shell,
                &chain_config,
                &ecosystem_config,
                &mut contracts,
                args,
            )
            .await?;
        }
        Deploy2ContractsOption::TimestampAsserter => {
            deploy_timestamp_asserter(
                shell,
                &chain_config,
                &ecosystem_config,
                &mut contracts,
                args,
            )
            .await?;
        }
        Deploy2ContractsOption::L2DAValidator => {
            deploy_l2_da_validator(
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
    with_broadcast: bool,
) -> anyhow::Result<()> {
    build_l2_contracts(shell.clone(), &ecosystem_config.link_to_code)?;
    call_forge(
        shell,
        chain_config,
        ecosystem_config,
        forge_args,
        signature,
        with_broadcast,
    )
    .await?;
    update_config(
        shell,
        &DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS.output(&chain_config.path_to_l1_foundry()),
    )?;
    Ok(())
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
        true,
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
        true,
    )
    .await
}

pub async fn deploy_multicall3(
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
        Some("runDeployMulticall3"),
        |shell, out| contracts_config.set_multicall3(&Multicall3Output::read(shell, out)?),
        true,
    )
    .await
}

pub async fn deploy_timestamp_asserter(
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
        Some("runDeployTimestampAsserter"),
        |shell, out| {
            contracts_config
                .set_timestamp_asserter_addr(&TimestampAsserterOutput::read(shell, out)?)
        },
        true,
    )
    .await
}

pub async fn deploy_l2_da_validator(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    _contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    build_and_deploy(
        shell,
        chain_config,
        ecosystem_config,
        forge_args,
        Some("runDeployL2DAValidator"),
        |shell, out| {
            // Now, we don't have a specific l2 da validator address
            Ok(())
        },
        true,
    )
    .await
}

pub async fn deploy_l2_contracts(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
    with_broadcast: bool,
) -> anyhow::Result<()> {
    build_and_deploy(
        shell,
        chain_config,
        ecosystem_config,
        forge_args,
        None,
        |shell, out| {
            contracts_config.set_l2_shared_bridge()?;
            contracts_config.set_default_l2_upgrade(&DefaultL2UpgradeOutput::read(shell, out)?)?;
            contracts_config.set_consensus_registry(&ConsensusRegistryOutput::read(shell, out)?)?;
            contracts_config.set_multicall3(&Multicall3Output::read(shell, out)?)?;
            contracts_config
                .set_timestamp_asserter_addr(&TimestampAsserterOutput::read(shell, out)?)?;
            Ok(())
        },
        with_broadcast,
    )
    .await
}

async fn call_forge(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    forge_args: ForgeScriptArgs,
    signature: Option<&str>,
    with_broadcast: bool,
) -> anyhow::Result<()> {
    let input = DeployL2ContractsInput::new(
        chain_config,
        &ecosystem_config.get_contracts_config()?,
        ecosystem_config.era_chain_id,
    )
    .await?;

    let foundry_contracts_path = chain_config.path_to_l1_foundry();
    let secrets = chain_config.get_secrets_config().await?;
    input.save(
        shell,
        DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS.input(&chain_config.path_to_l1_foundry()),
    )?;

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(
            &DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(secrets.l1_rpc_url()?);
    if with_broadcast {
        forge = forge.with_broadcast();
    }

    if let Some(signature) = signature {
        forge = forge.with_signature(signature);
    }

    forge = fill_forge_private_key(
        forge,
        Some(&ecosystem_config.get_wallets()?.governor),
        WalletOwner::Governor,
    )?;

    check_the_balance(&forge).await?;
    forge.run(shell)?;
    Ok(())
}
