use ethers::{abi::parse_abi, contract::BaseContract, types::H160};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    contracts::{
        build_da_contracts, build_l1_contracts, build_l2_contracts, build_system_contracts,
        install_yarn_dependencies,
    },
    forge::{Forge, ForgeScriptArgs},
    git, logger,
    spinner::Spinner,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::{
            input::{DeployL1Config, GenesisInput, InitialDeploymentConfig},
            output::DeployL1Output,
        },
        script_params::DEPLOY_ECOSYSTEM_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ContractsConfig, EcosystemConfig, GenesisConfig, ZkStackConfig, ZkStackConfigTrait,
    GENESIS_FILE,
};
use zkstack_cli_types::{L1Network, ProverMode};

use crate::{
    admin_functions::{accept_admin, accept_owner},
    commands::{
        ctm::args::InitNewCTMArgs,
        ecosystem::{
            args::init::EcosystemArgsFinal, create_configs::create_initial_deployments_config,
        },
    },
    messages::{
        MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_INITIALIZING_CTM, MSG_INTALLING_DEPS_SPINNER,
    },
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref DEPLOY_L1_FUNCTIONS: BaseContract = BaseContract::from(
        parse_abi(&["function runWithBridgehub(address bridgehub) public",]).unwrap(),
    );
}

pub async fn run(args: InitNewCTMArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    if args.update_submodules.is_none() || args.update_submodules == Some(true) {
        git::submodule_update(shell, &ecosystem_config.link_to_code())?;
    }

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    let init_ctm_args = args
        .clone()
        .fill_values_with_prompt(ecosystem_config.l1_network)
        .await?;

    logger::info(MSG_INITIALIZING_CTM);

    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    if !init_ctm_args.skip_contract_compilation_override {
        install_yarn_dependencies(shell, &ecosystem_config.link_to_code())?;
        build_da_contracts(shell, &ecosystem_config.contracts_path())?;
        build_l1_contracts(shell.clone(), &ecosystem_config.contracts_path())?;
        build_system_contracts(shell.clone(), &ecosystem_config.contracts_path())?;
        build_l2_contracts(shell.clone(), &ecosystem_config.contracts_path())?;
    }
    spinner.finish();

    let contracts = deploy_new_ctm_and_accept_admin(
        shell,
        &init_ctm_args.ecosystem,
        init_ctm_args.forge_args.clone(),
        &ecosystem_config,
        &initial_deployment_config,
        init_ctm_args.support_l2_legacy_shared_bridge_test,
        init_ctm_args.bridgehub_address, // Scripts are expected to consume 0 address for BH
        init_ctm_args.zksync_os,
    )
    .await?;
    contracts.save_with_base_path(shell, &ecosystem_config.config)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn deploy_new_ctm_and_accept_admin(
    shell: &Shell,
    ecosystem: &EcosystemArgsFinal,
    forge_args: ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    support_l2_legacy_shared_bridge_test: bool,
    bridgehub_address: Option<H160>,
    zksync_os: bool,
) -> anyhow::Result<ContractsConfig> {
    let l1_rpc_url = ecosystem.l1_rpc_url.clone();
    let spinner = Spinner::new(MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER);
    let contracts_config = deploy_new_ctm(
        shell,
        &forge_args,
        ecosystem_config,
        initial_deployment_config,
        &l1_rpc_url,
        None,
        true,
        support_l2_legacy_shared_bridge_test,
        bridgehub_address,
        zksync_os,
    )
    .await?;
    spinner.finish();

    accept_owner(
        shell,
        ecosystem_config.path_to_foundry_scripts(),
        contracts_config.l1.governance_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config
            .ecosystem_contracts
            .state_transition_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    accept_admin(
        shell,
        ecosystem_config.path_to_foundry_scripts(),
        contracts_config.l1.chain_admin_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config
            .ecosystem_contracts
            .state_transition_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    Ok(contracts_config)
}

#[allow(clippy::too_many_arguments)]
pub async fn deploy_new_ctm(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    l1_rpc_url: &str,
    sender: Option<String>,
    broadcast: bool,
    support_l2_legacy_shared_bridge_test: bool,
    bridgehub_address: Option<H160>,
    zksync_os: bool,
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
        zksync_os,
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
