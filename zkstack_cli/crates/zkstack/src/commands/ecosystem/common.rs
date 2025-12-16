use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::{
            input::{
                DeployErc20Config, DeployL1Config, Erc20DeploymentConfig, GenesisInput,
                InitialDeploymentConfig,
            },
            output::{DeployL1CoreContractsOutput, ERC20Tokens},
        },
        script_params::{
            DEPLOY_ECOSYSTEM_CORE_CONTRACTS_SCRIPT_PARAMS, DEPLOY_ERC20_SCRIPT_PARAMS,
        },
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ContractsConfigForDeployERC20, ContractsGenesisConfig, CoreContractsConfig, EcosystemConfig,
};
use zkstack_cli_types::{L1Network, ProverMode, VMOption};

use super::args::init::EcosystemInitArgsFinal;
use crate::{
    commands::chain::{self},
    messages::{msg_chain_load_err, msg_initializing_chain, MSG_DEPLOYING_ERC20_SPINNER},
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

#[allow(clippy::too_many_arguments)]
pub async fn deploy_l1_core_contracts(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    l1_rpc_url: &str,
    sender: Option<String>,
    broadcast: bool,
    support_l2_legacy_shared_bridge_test: bool,
    vm_option: VMOption,
) -> anyhow::Result<CoreContractsConfig> {
    let deploy_config_path = DEPLOY_ECOSYSTEM_CORE_CONTRACTS_SCRIPT_PARAMS
        .input(&config.path_to_foundry_scripts_for_ctm(vm_option));
    let genesis_config_path = config.default_genesis_path(vm_option);
    let default_genesis_config = ContractsGenesisConfig::read(shell, &genesis_config_path).await?;
    let default_genesis_input = GenesisInput::new(&default_genesis_config, vm_option)?;
    let wallets_config = config.get_wallets()?;
    // For deploying ecosystem we only need genesis batch params
    let deploy_config = DeployL1Config::new(
        &default_genesis_input,
        &wallets_config,
        initial_deployment_config,
        config.era_chain_id,
        config.prover_version == ProverMode::NoProofs,
        config.l1_network,
        support_l2_legacy_shared_bridge_test,
        vm_option,
    );

    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry_scripts_for_ctm(vm_option))
        .script(
            &DEPLOY_ECOSYSTEM_CORE_CONTRACTS_SCRIPT_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url.to_string());

    if config.l1_network == L1Network::Localhost {
        // It's a kludge for reth, just because it doesn't behave properly with large amount of txs
        forge = forge.with_slow();
    }

    if let Some(address) = sender {
        forge = forge.with_sender(address);
    } else {
        forge = fill_forge_private_key(
            forge,
            wallets_config.deployer.as_ref(),
            WalletOwner::Deployer,
        )?;
    }

    if broadcast {
        forge = forge.with_broadcast();
        check_the_balance(&forge).await?;
    }

    forge.run(shell)?;

    let script_output = DeployL1CoreContractsOutput::read(
        shell,
        DEPLOY_ECOSYSTEM_CORE_CONTRACTS_SCRIPT_PARAMS
            .output(&config.path_to_foundry_scripts_for_ctm(vm_option)),
    )?;
    let mut contracts_config = CoreContractsConfig::default();
    contracts_config.update_from_l1_output(&script_output);

    Ok(contracts_config)
}

pub async fn deploy_erc20(
    shell: &Shell,
    erc20_deployment_config: &Erc20DeploymentConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &ContractsConfigForDeployERC20,
    forge_args: ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<ERC20Tokens> {
    // Deploy ERC20 tokens is always from non zksync os
    let vm_option = VMOption::EraVM;
    let deploy_config_path = DEPLOY_ERC20_SCRIPT_PARAMS
        .input(&ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option));
    let wallets = ecosystem_config.get_wallets()?;
    DeployErc20Config::new(
        erc20_deployment_config,
        contracts_config,
        vec![
            wallets.governor.address,
            wallets.operator.address,
            wallets.blob_operator.address,
        ],
    )
    .save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option))
        .script(&DEPLOY_ERC20_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        ecosystem_config.get_wallets()?.deployer.as_ref(),
        WalletOwner::Deployer,
    )?;

    let spinner = Spinner::new(MSG_DEPLOYING_ERC20_SPINNER);
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    spinner.finish();

    let result = ERC20Tokens::read(
        shell,
        DEPLOY_ERC20_SCRIPT_PARAMS
            .output(&ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option)),
    )?;
    result.save_with_base_path(shell, &ecosystem_config.config)?;
    Ok(result)
}

pub async fn init_chains(
    mut args: EcosystemInitArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<Vec<String>> {
    // If the name of chain passed then we deploy exactly this chain otherwise deploy all chains
    let list_of_chains = if let Some(name) = global_config().chain_name.clone() {
        vec![name]
    } else {
        ecosystem_config.list_of_chains()
    };
    // Set default values for dev mode
    let mut deploy_paymaster = args.deploy_paymaster;
    let genesis_args = &mut args.genesis_args;
    if args.dev {
        deploy_paymaster = Some(deploy_paymaster.unwrap_or(true));
        if let Some(genesis) = genesis_args {
            genesis.dev = true;
        }
    }
    // Can't initialize multiple chains with the same DB
    if list_of_chains.len() > 1 {
        if let Some(genesis) = genesis_args {
            genesis.reset_db_names();
        }
    }
    // Initialize chains
    for chain_name in &list_of_chains {
        logger::info(msg_initializing_chain(chain_name));
        let chain_config = ecosystem_config
            .load_chain(Some(chain_name.clone()))
            .context(msg_chain_load_err(chain_name))?;

        let chain_init_args = chain::args::init::InitArgs {
            forge_args: args.forge_args.clone(),
            server_db_url: genesis_args.as_ref().and_then(|a| a.server_db_url.clone()),
            server_db_name: genesis_args.as_ref().and_then(|a| a.server_db_name.clone()),
            dont_drop: genesis_args
                .as_ref()
                .map(|a| a.dont_drop)
                .unwrap_or_default(),
            deploy_paymaster,
            l1_rpc_url: Some(args.l1_rpc_url.clone()),
            no_port_reallocation: args.no_port_reallocation,
            dev: args.dev,
            validium_args: args.validium_args.clone(),
            server_command: genesis_args.as_ref().and_then(|a| a.server_command.clone()),
            make_permanent_rollup: args.make_permanent_rollup,
            no_genesis: genesis_args.is_none(),
            skip_priority_txs: args.skip_priority_txs,
            pause_deposits: args.pause_deposits,
        };
        let final_chain_init_args = chain_init_args.fill_values_with_prompt(&chain_config);

        chain::init::init(
            &final_chain_init_args,
            shell,
            ecosystem_config,
            &chain_config,
        )
        .await?;
    }
    Ok(list_of_chains)
}
