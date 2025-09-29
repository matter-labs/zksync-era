use ethers::{abi::parse_abi, contract::BaseContract, types::H160};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    contracts::rebuild_all_contracts,
    forge::{Forge, ForgeScriptArgs},
    git, logger,
    spinner::Spinner,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::{
            input::{DeployL1Config, GenesisInput, InitialDeploymentConfig},
            output::DeployCTMOutput,
        },
        script_params::DEPLOY_CTM_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    CoreContractsConfig, EcosystemConfig, GenesisConfig, ZkStackConfig, GENESIS_FILE,
};
use zkstack_cli_types::{L1Network, ProverMode};

use crate::{
    admin_functions::{accept_admin, accept_owner},
    commands::{
        ctm::args::InitNewCTMArgs, ecosystem::create_configs::create_initial_deployments_config,
    },
    messages::{
        MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR,
        MSG_INITIALIZING_CTM, MSG_INTALLING_DEPS_SPINNER,
    },
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref DEPLOY_CTM_FUNCTIONS: BaseContract = BaseContract::from(
        parse_abi(&["function runWithBridgehub(address bridgehub, bool reuseGovAndAdmin) public",])
            .unwrap(),
    );
}
pub async fn run(args: InitNewCTMArgs, shell: &Shell) -> anyhow::Result<()> {
    let zksync_os = args.common.zksync_os;
    let mut ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    let init_ctm_args = args
        .clone()
        .fill_values_with_prompt(ecosystem_config.l1_network)
        .await?;

    if let Some(path) = init_ctm_args.contracts_src_path {
        if !path.exists() || !path.is_dir() {
            return Err(anyhow::anyhow!(MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR));
        }
        logger::info(format!("Using contracts source path: {}", path.display()));
        ecosystem_config.set_sources_path(
            path,
            init_ctm_args.default_configs_src_path.unwrap(),
            zksync_os,
        );
        ecosystem_config.save_with_base_path(shell, ".")?;
    }

    logger::info(MSG_INITIALIZING_CTM);

    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    spinner.finish();

    let bridgehub_address = if let Some(addr) = init_ctm_args.bridgehub_address {
        addr
    } else {
        ecosystem_config
            .get_contracts_config()?
            .core_ecosystem_contracts
            .bridgehub_proxy_addr
    };

    if args.common.update_submodules {
        git::submodule_update(shell, &ecosystem_config.link_to_code())?;
    }
    if !args.common.skip_build_dependencies {
        rebuild_all_contracts(shell, &ecosystem_config.contracts_path_for_ctm(zksync_os))?;
    }

    let contracts = deploy_new_ctm_and_accept_admin(
        shell,
        init_ctm_args.ecosystem.l1_rpc_url.clone(),
        &init_ctm_args.forge_args,
        &ecosystem_config,
        &initial_deployment_config,
        init_ctm_args.support_l2_legacy_shared_bridge_test,
        bridgehub_address,
        init_ctm_args.zksync_os,
        init_ctm_args.reuse_gov_and_admin,
    )
    .await?;
    contracts.save_with_base_path(shell, &ecosystem_config.config)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn deploy_new_ctm_and_accept_admin(
    shell: &Shell,
    l1_rpc_url: String,
    forge_args: &ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    support_l2_legacy_shared_bridge_test: bool,
    bridgehub_address: H160,
    zksync_os: bool,
    reuse_gov_and_admin: bool,
) -> anyhow::Result<CoreContractsConfig> {
    let spinner = Spinner::new(MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER);
    let contracts_config = deploy_new_ctm(
        shell,
        forge_args,
        ecosystem_config,
        initial_deployment_config,
        &l1_rpc_url,
        None,
        true,
        support_l2_legacy_shared_bridge_test,
        bridgehub_address,
        zksync_os,
        reuse_gov_and_admin,
    )
    .await?;
    spinner.finish();

    let ctm = contracts_config.ctm(zksync_os);
    accept_owner(
        shell,
        ecosystem_config.path_to_foundry_scripts_for_ctm(zksync_os),
        contracts_config.l1.governance_addr,
        &ecosystem_config.get_wallets()?.governor,
        ctm.state_transition_proxy_addr,
        forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    accept_admin(
        shell,
        ecosystem_config.path_to_foundry_scripts_for_ctm(zksync_os),
        contracts_config.l1.chain_admin_addr,
        &ecosystem_config.get_wallets()?.governor,
        ctm.state_transition_proxy_addr,
        forge_args,
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
    bridgehub_address: H160,
    zksync_os: bool,
    reuse_gov_and_admin: bool,
) -> anyhow::Result<CoreContractsConfig> {
    let mut contracts_config = config.get_contracts_config()?;
    let deploy_config_path =
        DEPLOY_CTM_SCRIPT_PARAMS.input(&config.path_to_foundry_scripts_for_ctm(zksync_os));
    let genesis_config_path = config
        .default_configs_path_for_ctm(zksync_os)
        .join(GENESIS_FILE);
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
        .encode("runWithBridgehub", (bridgehub_address, reuse_gov_and_admin))
        .unwrap();

    let mut forge = Forge::new(&config.path_to_foundry_scripts_for_ctm(zksync_os))
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
        DEPLOY_CTM_SCRIPT_PARAMS.output(&config.path_to_foundry_scripts_for_ctm(zksync_os)),
    )?;
    contracts_config.update_from_ctm_output(&script_output, zksync_os);

    Ok(contracts_config)
}
