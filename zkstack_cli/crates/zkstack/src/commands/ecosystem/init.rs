use std::{path::PathBuf, str::FromStr};

use xshell::Shell;
use zkstack_cli_common::{logger, spinner::Spinner, Prompt};
use zkstack_cli_config::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig,
    traits::{FileConfigWithDefaultName, SaveConfigWithBasePath},
    ContractsConfig, CoreContractsConfig, EcosystemConfig, ZkStackConfig,
};
use zkstack_cli_types::{L1Network, ProverMode, VMOption};

use super::{
    args::init::{EcosystemInitArgs, EcosystemInitArgsFinal},
    common::init_chains,
    setup_observability,
};
use crate::{
    commands::ecosystem::{
        common::deploy_erc20,
        create_configs::{create_erc20_deployment_config, create_initial_deployments_config},
    },
    messages::{
        msg_ecosystem_initialized, msg_ecosystem_no_found_preexisting_contract,
        MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_DEPLOYING_ERC20,
        MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR, MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT,
        MSG_INITIALIZING_ECOSYSTEM, MSG_INTALLING_DEPS_SPINNER,
    },
    utils::protocol_ops::{EcosystemInitProtocolOpsArgs, ProtocolOpsRunner},
};

pub async fn run(args: EcosystemInitArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    let final_ecosystem_args = args
        .fill_values_with_prompt(ecosystem_config.l1_network)
        .await?;

    logger::info(MSG_INITIALIZING_ECOSYSTEM);

    if final_ecosystem_args.observability {
        setup_observability::run(shell)?;
    }

    let contracts_config = init_ecosystem(
        &final_ecosystem_args,
        shell,
        &ecosystem_config,
        &initial_deployment_config,
    )
    .await?;

    if final_ecosystem_args.deploy_erc20 {
        logger::info(MSG_DEPLOYING_ERC20);
        let erc20_deployment_config = match ecosystem_config.get_erc20_deployment_config() {
            Ok(config) => config,
            Err(_) => create_erc20_deployment_config(shell, &ecosystem_config.config)?,
        };
        deploy_erc20(
            shell,
            &erc20_deployment_config,
            &ecosystem_config,
            &contracts_config.into(),
            final_ecosystem_args.forge_args.clone(),
            final_ecosystem_args.l1_rpc_url.clone(),
        )
        .await?;
    }

    // Initialize chain(s)
    let mut chains: Vec<String> = vec![];
    if !final_ecosystem_args.ecosystem_only {
        chains = init_chains(final_ecosystem_args.clone(), shell, &ecosystem_config).await?;
    }
    logger::outro(msg_ecosystem_initialized(&chains.join(",")));

    Ok(())
}

async fn init_ecosystem(
    init_args: &EcosystemInitArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    _initial_deployment_config: &InitialDeploymentConfig,
) -> anyhow::Result<CoreContractsConfig> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    spinner.finish();

    let contracts = if !init_args.deploy_ecosystem {
        return_ecosystem_contracts(
            shell,
            init_args.ecosystem_contracts_path.clone(),
            ecosystem_config,
        )
        .await?
    } else {
        let contracts = deploy_ecosystem(
            shell,
            init_args.l1_rpc_url.clone(),
            ecosystem_config,
            init_args.support_l2_legacy_shared_bridge_test,
            init_args.vm_option,
        )
        .await?;
        contracts.save_with_base_path(shell, &ecosystem_config.config)?;
        contracts
    };
    Ok(contracts)
}

async fn return_ecosystem_contracts(
    shell: &Shell,
    ecosystem_contracts_path: Option<PathBuf>,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<CoreContractsConfig> {
    let ecosystem_contracts_path = match &ecosystem_contracts_path {
        Some(path) => Some(path.clone()),
        None => {
            let input_path: String = Prompt::new(MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT)
                .allow_empty()
                .validate_with(|val: &String| {
                    if val.is_empty() {
                        return Ok(());
                    }
                    PathBuf::from_str(val)
                        .map(|_| ())
                        .map_err(|_| MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR.to_string())
                })
                .ask();
            if input_path.is_empty() {
                None
            } else {
                Some(input_path.into())
            }
        }
    };

    let ecosystem_preexisting_configs_path =
        ecosystem_config
            .get_preexisting_configs_path()
            .join(format!(
                "{}.yaml",
                ecosystem_config.l1_network.to_string().to_lowercase()
            ));

    // currently there are not some preexisting ecosystem contracts in
    // chains, so we need check if this file exists.
    if ecosystem_contracts_path.is_none() && !ecosystem_preexisting_configs_path.exists() {
        anyhow::bail!(msg_ecosystem_no_found_preexisting_contract(
            &ecosystem_config.l1_network.to_string()
        ))
    }

    let ecosystem_contracts_path =
        ecosystem_contracts_path.unwrap_or_else(|| match ecosystem_config.l1_network {
            L1Network::Localhost => {
                ContractsConfig::get_path_with_base_path(&ecosystem_config.config)
            }
            L1Network::Sepolia | L1Network::Holesky | L1Network::Mainnet => {
                ecosystem_preexisting_configs_path
            }
        });

    // We don't have a zksync os preexisting contracts config, so we can assume
    // that it's always false during fallback
    CoreContractsConfig::read_with_fallback(shell, ecosystem_contracts_path, VMOption::EraVM)
}

async fn deploy_ecosystem(
    shell: &Shell,
    l1_rpc_url: String,
    ecosystem_config: &EcosystemConfig,
    support_l2_legacy_shared_bridge_test: bool,
    vm_option: VMOption,
) -> anyhow::Result<CoreContractsConfig> {
    let spinner = Spinner::new(MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER);
    let wallets = ecosystem_config.get_wallets()?;

    let deployer = wallets
        .deployer
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Deployer wallet is required for ecosystem init"))?;
    let deployer_pk = deployer
        .private_key_h256()
        .ok_or_else(|| anyhow::anyhow!("Deployer wallet private key is required"))?;

    // Governor private key is needed when governor != deployer (for accepting ownership)
    let governor_pk = wallets.governor.private_key_h256();

    let runner = ProtocolOpsRunner::new(ecosystem_config);
    let args = EcosystemInitProtocolOpsArgs {
        private_key: deployer_pk,
        owner: wallets.governor.address,
        owner_private_key: governor_pk,
        l1_rpc_url,
        era_chain_id: ecosystem_config.era_chain_id.as_u64(),
        vm_type: vm_option,
        with_testnet_verifier: ecosystem_config.prover_version == ProverMode::NoProofs,
        with_legacy_bridge: support_l2_legacy_shared_bridge_test,
    };
    let output = runner.ecosystem_init(shell, &args)?;
    spinner.finish();

    Ok(output.to_core_contracts_config(vm_option))
}
