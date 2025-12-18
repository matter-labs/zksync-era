use std::path::Path;

use anyhow::Context;
use ethers::contract::BaseContract;
use xshell::Shell;
use zkstack_cli_common::{
    contracts::build_l2_contracts,
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_l2_contracts::output::{
            ConsensusRegistryOutput, DefaultL2UpgradeOutput, Multicall3Output,
            TimestampAsserterOutput,
        },
        script_params::DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, DAValidatorType, EcosystemConfig, ZkStackConfig,
    ZkStackConfigTrait,
};
use zksync_basic_types::commitment::L1BatchCommitmentMode;

use crate::{
    abi::IDEPLOYL2CONTRACTSABI_ABI,
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

    let l1_rpc_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    match deploy_option {
        Deploy2ContractsOption::All => {
            deploy_l2_contracts(
                shell,
                &chain_config,
                &ecosystem_config,
                &mut contracts,
                args,
                true,
                l1_rpc_url,
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
                l1_rpc_url,
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
                l1_rpc_url,
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
                l1_rpc_url,
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
                l1_rpc_url,
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
                l1_rpc_url,
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
#[allow(clippy::too_many_arguments)]
async fn build_and_deploy(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    forge_args: ForgeScriptArgs,
    signature: Option<&str>,
    mut update_config: impl FnMut(&Shell, &Path) -> anyhow::Result<()>,
    with_broadcast: bool,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    build_l2_contracts(shell.clone(), &chain_config.contracts_path())?;
    call_forge(
        shell,
        chain_config,
        ecosystem_config,
        forge_args,
        signature,
        with_broadcast,
        l1_rpc_url,
    )
    .await?;
    update_config(
        shell,
        &DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS.output(&chain_config.path_to_foundry_scripts()),
    )?;
    Ok(())
}

pub async fn deploy_upgrader(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
    l1_rpc_url: String,
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
        l1_rpc_url,
    )
    .await
}

pub async fn deploy_consensus_registry(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
    l1_rpc_url: String,
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
        l1_rpc_url,
    )
    .await
}

pub async fn deploy_multicall3(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    build_and_deploy(
        shell,
        chain_config,
        ecosystem_config,
        forge_args,
        Some("runDeployMulticall3"),
        |shell, out| contracts_config.set_multicall3(&Multicall3Output::read(shell, out)?),
        true,
        l1_rpc_url,
    )
    .await
}

pub async fn deploy_timestamp_asserter(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
    l1_rpc_url: String,
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
        l1_rpc_url,
    )
    .await
}

pub async fn deploy_l2_da_validator(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    _contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    build_and_deploy(
        shell,
        chain_config,
        ecosystem_config,
        forge_args,
        Some("runDeployL2DAValidator"),
        |_shell, _out| {
            // Now, we don't have a specific l2 da validator address
            Ok(())
        },
        true,
        l1_rpc_url,
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
    l1_rpc_url: String,
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
        l1_rpc_url,
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
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let foundry_contracts_path = chain_config.path_to_foundry_scripts();

    // Extract parameters directly from configs
    let contracts_config = chain_config.get_contracts_config()?;
    let ecosystem_contracts = ecosystem_config.get_contracts_config()?;
    let wallets = chain_config.get_wallets_config()?;

    let bridgehub = contracts_config.ecosystem_contracts.bridgehub_proxy_addr;
    let chain_id = chain_config.chain_id.as_u64();
    let governance = ecosystem_contracts.l1.governance_addr;
    let consensus_registry_owner = wallets.governor.address;
    let da_validator_type = get_da_validator_type(chain_config).await? as u64;

    // Encode calldata with all parameters
    let deploy_l2_contract = BaseContract::from(IDEPLOYL2CONTRACTSABI_ABI.clone());

    let calldata = deploy_l2_contract
        .encode(
            "run",
            (
                bridgehub,
                chain_id,
                governance,
                consensus_registry_owner,
                da_validator_type,
            ),
        )
        .unwrap();

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(
            &DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_calldata(&calldata);

    if with_broadcast {
        forge = forge.with_broadcast();
    }

    if let Some(signature) = signature {
        forge = forge.with_signature(signature);
    } else {
        // When no signature is provided, we need to encode calldata for the run function
        // kl todo this might be wrong
        let deploy_l2_contract = BaseContract::from(IDEPLOYL2CONTRACTSABI_ABI.clone());
        let calldata = deploy_l2_contract
            .encode(
                "run",
                (
                    bridgehub,
                    chain_id,
                    governance,
                    consensus_registry_owner,
                    da_validator_type,
                ),
            )
            .unwrap();
        forge = forge.with_calldata(&calldata);
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

async fn get_da_validator_type(config: &ChainConfig) -> anyhow::Result<DAValidatorType> {
    let da_client_type = config
        .get_general_config()
        .await
        .map(|c| c.da_client_type())
        .unwrap_or_default();

    match (
        config.l1_batch_commit_data_generator_mode,
        da_client_type.as_deref(),
    ) {
        (L1BatchCommitmentMode::Rollup, _) => Ok(DAValidatorType::Rollup),
        (L1BatchCommitmentMode::Validium, None | Some("NoDA")) => Ok(DAValidatorType::NoDA),
        (L1BatchCommitmentMode::Validium, Some("Avail")) => Ok(DAValidatorType::Avail),
        (L1BatchCommitmentMode::Validium, Some("Eigen")) => Ok(DAValidatorType::NoDA), // TODO: change to EigenDA for M1
        _ => anyhow::bail!("DAValidatorType is not supported"),
    }
}
