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
            output::{DeployCTMOutput, DeployL1CoreContractsOutput, ERC20Tokens},
        },
        script_params::{
            DEPLOY_CTM_SCRIPT_PARAMS, DEPLOY_ECOSYSTEM_CORE_CONTRACTS_SCRIPT_PARAMS,
            DEPLOY_ERC20_SCRIPT_PARAMS, REGISTER_CTM_SCRIPT_PARAMS,
        },
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ContractsConfig, ContractsConfigForDeployERC20, CoreContractsConfig, EcosystemConfig,
    GenesisConfig, ZkStackConfigTrait, GENESIS_FILE,
};
use zkstack_cli_types::{L1Network, ProverMode};

use super::args::init::EcosystemInitArgsFinal;
use crate::{
    admin_functions::{AdminScriptOutput, AdminScriptOutputInner},
    commands::chain::{self},
    messages::{msg_chain_load_err, msg_initializing_chain, MSG_DEPLOYING_ERC20_SPINNER},
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref DEPLOY_CTM_FUNCTIONS: BaseContract = BaseContract::from(
        parse_abi(&["function runWithBridgehub(address bridgehub, bool reuseGovAndAdmin) public",]).unwrap(),
    );
    static ref REGISTER_CTM_FUNCTIONS: BaseContract =
        BaseContract::from(parse_abi(&["function registerCTM(address bridgehub, address chainTypeManagerProxy, bool shouldSend) public",]).unwrap(),);
}

#[allow(clippy::too_many_arguments)]
pub async fn deploy_ctm(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    l1_rpc_url: &str,
    sender: Option<String>,
    broadcast: bool,
    support_l2_legacy_shared_bridge_test: bool,
    bridgehub_address: H160,
    zksync_os: bool,
    reuse_gov_and_admin: bool,
) -> anyhow::Result<ContractsConfig> {
    let deploy_config_path = DEPLOY_CTM_SCRIPT_PARAMS.input(&config.path_to_foundry_scripts());
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
        zksync_os,
    );
    deploy_config.save(shell, deploy_config_path)?;

    let calldata = DEPLOY_CTM_FUNCTIONS
        .encode("runWithBridgehub", (bridgehub_address, reuse_gov_and_admin)) // Script works with zero address
        .unwrap();

    let mut forge = Forge::new(&config.path_to_foundry_scripts())
        .script(&DEPLOY_CTM_SCRIPT_PARAMS.script(), forge_args.clone())
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

    let script_output = DeployCTMOutput::read(
        shell,
        DEPLOY_CTM_SCRIPT_PARAMS.output(&config.path_to_foundry_scripts()),
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
) -> anyhow::Result<CoreContractsConfig> {
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
        // ZKSync OS flag is not used in core contracts deployment
        false,
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

    let script_output = DeployL1CoreContractsOutput::read(
        shell,
        DEPLOY_ECOSYSTEM_CORE_CONTRACTS_SCRIPT_PARAMS.output(&config.path_to_foundry_scripts()),
    )?;
    let mut contracts_config = CoreContractsConfig::default();
    contracts_config.update_from_l1_output(&script_output);

    Ok(contracts_config)
}

#[allow(clippy::too_many_arguments)]
pub async fn register_ctm_on_existing_bh(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    config: &EcosystemConfig,
    l1_rpc_url: &str,
    sender: Option<String>,
    bridgehub_address: H160,
    ctm_address: H160,
    only_save_calldata: bool,
) -> anyhow::Result<AdminScriptOutput> {
    let wallets_config = config.get_wallets()?;

    let calldata = REGISTER_CTM_FUNCTIONS
        .encode(
            "registerCTM",
            (bridgehub_address, ctm_address, !only_save_calldata),
        )
        .unwrap();

    let mut forge = Forge::new(&config.path_to_foundry_scripts())
        .script(&REGISTER_CTM_SCRIPT_PARAMS.script(), forge_args.clone())
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
        forge =
            fill_forge_private_key(forge, Some(&wallets_config.governor), WalletOwner::Governor)?;
    }

    if !only_save_calldata {
        forge = forge.with_broadcast();
        check_the_balance(&forge).await?;
    }

    let output_path = REGISTER_CTM_SCRIPT_PARAMS.output(&config.path_to_foundry_scripts());
    forge.run(shell)?;

    Ok(AdminScriptOutputInner::read(shell, output_path)?.into())
}

pub async fn deploy_erc20(
    shell: &Shell,
    erc20_deployment_config: &Erc20DeploymentConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &ContractsConfigForDeployERC20,
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
        deploy_paymaster = Some(true);
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
            l1_rpc_url: Some(args.ecosystem.l1_rpc_url.clone()),
            no_port_reallocation: args.no_port_reallocation,
            update_submodules: args.update_submodules,
            dev: args.dev,
            validium_args: args.validium_args.clone(),
            server_command: genesis_args.as_ref().and_then(|a| a.server_command.clone()),
            make_permanent_rollup: args.make_permanent_rollup,
            no_genesis: genesis_args.is_none(),
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
