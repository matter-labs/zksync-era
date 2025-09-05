use anyhow::Context;
use ethers::{abi::parse_abi, contract::BaseContract, types::H160};
use lazy_static::lazy_static;
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
            output::{DeployL1Output, ERC20Tokens},
        },
        script_params::{
            DEPLOY_ECOSYSTEM_CORE_CONTRACTS_SCRIPT_PARAMS, DEPLOY_ECOSYSTEM_SCRIPT_PARAMS,
            DEPLOY_ERC20_SCRIPT_PARAMS, REGISTER_CTM_SCRIPT_PARAMS,
        },
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ContractsConfig, EcosystemConfig, GenesisConfig, ZkStackConfigTrait, GENESIS_FILE,
};
use zkstack_cli_types::{L1Network, ProverMode};

use super::args::init::{EcosystemInitArgs, EcosystemInitArgsFinal};
use crate::{
    commands::chain::{self},
    messages::{msg_chain_load_err, msg_initializing_chain, MSG_DEPLOYING_ERC20_SPINNER},
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref DEPLOY_L1_FUNCTIONS: BaseContract = BaseContract::from(
        parse_abi(&["function runWithBridgehub(address bridgehub) public",]).unwrap(),
    );
}

#[allow(clippy::too_many_arguments)]
pub async fn deploy_l1(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    l1_rpc_url: &str,
    sender: Option<String>,
    broadcast: bool,
    support_l2_legacy_shared_bridge_test: bool,
    bridgehub_address: Option<H160>,
) -> anyhow::Result<ContractsConfig> {
    let deploy_config_path =
        DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.input(&config.path_to_foundry_scripts());
    let genesis_config_path = config.default_configs_path().join(GENESIS_FILE);
    let default_genesis_config = GenesisConfig::read(shell, &genesis_config_path).await?;
    let default_genesis_input = GenesisInput::new(&default_genesis_config)?;

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
    );
    deploy_config.save(shell, deploy_config_path)?;

    let calldata = DEPLOY_L1_FUNCTIONS
        .encode("runWithBridgehub", (bridgehub_address.unwrap_or_default(),)) // Script works with zero address
        .unwrap();

    let mut forge = Forge::new(&config.path_to_foundry_scripts())
        .script(&DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_calldata(&calldata)
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

    let script_output = DeployL1Output::read(
        shell,
        DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.output(&config.path_to_foundry_scripts()),
    )?;
    let mut contracts_config = ContractsConfig::default();
    contracts_config.update_from_l1_output(&script_output);

    Ok(contracts_config)
}

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
) -> anyhow::Result<ContractsConfig> {
    let deploy_config_path =
        DEPLOY_ECOSYSTEM_CORE_CONTRACTS_SCRIPT_PARAMS.input(&config.path_to_foundry_scripts());
    let genesis_config_path = config.default_configs_path().join(GENESIS_FILE);
    let default_genesis_config = GenesisConfig::read(shell, &genesis_config_path).await?;
    let default_genesis_input = GenesisInput::new(&default_genesis_config)?;
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
    );

    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry_scripts())
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

    let script_output = DeployL1Output::read(
        shell,
        DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.output(&config.path_to_foundry_scripts()),
    )?;
    let mut contracts_config = ContractsConfig::default();
    contracts_config.update_from_l1_output(&script_output);

    Ok(contracts_config)
}

pub async fn register_ctm_on_existing_bh(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    config: &EcosystemConfig,
    l1_rpc_url: &str,
    sender: Option<String>,
    broadcast: bool,
) -> anyhow::Result<()> {
    let wallets_config = config.get_wallets()?;

    let mut forge = Forge::new(&config.path_to_foundry_scripts())
        .script(&REGISTER_CTM_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url.to_string());

    if config.l1_network == L1Network::Localhost {
        // It's a kludge for reth, just because it doesn't behave properly with large amount of txs
        forge = forge.with_slow();
    }

    if let Some(address) = sender {
        forge = forge.with_sender(address);
    } else {
        forge =
            fill_forge_private_key(forge, Some(&wallets_config.governor), WalletOwner::Governor)?;
    }

    if broadcast {
        forge = forge.with_broadcast();
        check_the_balance(&forge).await?;
    }

    forge.run(shell)?;

    Ok(())
}

pub async fn deploy_erc20(
    shell: &Shell,
    erc20_deployment_config: &Erc20DeploymentConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &ContractsConfig,
    forge_args: ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<ERC20Tokens> {
    let deploy_config_path =
        DEPLOY_ERC20_SCRIPT_PARAMS.input(&ecosystem_config.path_to_foundry_scripts());
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

    let mut forge = Forge::new(&ecosystem_config.path_to_foundry_scripts())
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
        DEPLOY_ERC20_SCRIPT_PARAMS.output(&ecosystem_config.path_to_foundry_scripts()),
    )?;
    result.save_with_base_path(shell, &ecosystem_config.config)?;
    Ok(result)
}

pub async fn init_chains(
    init_args: &EcosystemInitArgs,
    final_init_args: &EcosystemInitArgsFinal,
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
    let mut deploy_paymaster = init_args.deploy_paymaster;
    let mut genesis_args = init_args.get_genesis_args().clone();
    if final_init_args.dev {
        deploy_paymaster = Some(true);
        genesis_args.dev = true;
    }
    // Can't initialize multiple chains with the same DB
    if list_of_chains.len() > 1 {
        genesis_args.reset_db_names();
    }
    // Initialize chains
    for chain_name in &list_of_chains {
        logger::info(msg_initializing_chain(chain_name));
        let chain_config = ecosystem_config
            .load_chain(Some(chain_name.clone()))
            .context(msg_chain_load_err(chain_name))?;

        let chain_init_args = chain::args::init::InitArgs {
            forge_args: final_init_args.forge_args.clone(),
            server_db_url: genesis_args.server_db_url.clone(),
            server_db_name: genesis_args.server_db_name.clone(),
            dont_drop: genesis_args.dont_drop,
            deploy_paymaster,
            l1_rpc_url: Some(final_init_args.ecosystem.l1_rpc_url.clone()),
            no_port_reallocation: final_init_args.no_port_reallocation,
            update_submodules: init_args.update_submodules,
            dev: final_init_args.dev,
            validium_args: final_init_args.validium_args.clone(),
            server_command: genesis_args.server_command.clone(),
            make_permanent_rollup: init_args.make_permanent_rollup,
            no_genesis: init_args.no_genesis,
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
