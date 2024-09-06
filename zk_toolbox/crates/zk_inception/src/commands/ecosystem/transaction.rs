use anyhow::Context;
use common::{forge::Forge, git, logger, spinner::Spinner};
use config::{
    forge_interface::{
        deploy_ecosystem::input::DeployL1Config, script_params::DEPLOY_ECOSYSTEM_SCRIPT_PARAMS,
    },
    traits::{ReadConfigWithBasePath, SaveConfig},
    EcosystemConfig, GenesisConfig,
};
use types::ProverMode;
use xshell::Shell;

use super::{
    args::transaction::EcosystemTransactionArgs,
    create_configs::create_initial_deployments_config,
    utils::{build_system_contracts, install_yarn_dependencies},
};
use crate::messages::{
    MSG_BUILDING_ECOSYSTEM, MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER,
    MSG_ECOSYSTEM_TXN_IMPOSSIBLE_TO_READ_GENESIS_CONFIG, MSG_ECOSYSTEM_TXN_OUTRO,
    MSG_ECOSYSTEM_TXN_OUT_PATH_INVALID_ERR, MSG_INTALLING_DEPS_SPINNER,
    MSG_PREPARING_CONFIG_SPINNER, MSG_WRITING_OUTPUT_FILES_SPINNER,
};

const DEPLOY_TRANSACTIONS_FILE_SRC: &str =
    "contracts/l1-contracts/broadcast/DeployL1.s.sol/9/dry-run/run-latest.json";
const DEPLOY_TRANSACTIONS_FILE_DST: &str = "deploy-l1-txns.json";

const SCRIPT_CONFIG_FILE_SRC: &str = "contracts/l1-contracts/script-config/config-deploy-l1.toml";
const SCRIPT_CONFIG_FILE_DST: &str = "config-deploy-l1.toml";

pub async fn run(args: EcosystemTransactionArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    logger::info(MSG_BUILDING_ECOSYSTEM);

    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    install_yarn_dependencies(shell, &ecosystem_config.link_to_code)?;
    build_system_contracts(shell, &ecosystem_config.link_to_code)?;
    spinner.finish();

    let spinner = Spinner::new(MSG_PREPARING_CONFIG_SPINNER);
    let default_genesis_config =
        GenesisConfig::read_with_base_path(shell, ecosystem_config.get_default_configs_path())
            .context(MSG_ECOSYSTEM_TXN_IMPOSSIBLE_TO_READ_GENESIS_CONFIG)?;

    let wallets_config = ecosystem_config.get_wallets()?;
    // For deploying ecosystem we only need genesis batch params
    let deploy_config = DeployL1Config::new(
        &default_genesis_config,
        &wallets_config,
        &initial_deployment_config,
        ecosystem_config.era_chain_id,
        ecosystem_config.prover_version == ProverMode::NoProofs,
    );
    let deploy_config_path = DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.input(&ecosystem_config.link_to_code);
    deploy_config.save(shell, deploy_config_path)?;
    spinner.finish();

    let spinner = Spinner::new(MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER);
    let forge = Forge::new(&ecosystem_config.path_to_foundry())
        .script(
            &DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.script(),
            args.forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(args.l1_rpc_url)
        .with_sender(args.sender);

    forge.run(shell)?;
    spinner.finish();

    let spinner = Spinner::new(MSG_WRITING_OUTPUT_FILES_SPINNER);
    shell
        .create_dir(&args.out)
        .context(MSG_ECOSYSTEM_TXN_OUT_PATH_INVALID_ERR)?;

    shell.copy_file(
        ecosystem_config
            .link_to_code
            .join(DEPLOY_TRANSACTIONS_FILE_SRC),
        args.out.join(DEPLOY_TRANSACTIONS_FILE_DST),
    )?;

    shell.copy_file(
        ecosystem_config.link_to_code.join(SCRIPT_CONFIG_FILE_SRC),
        args.out.join(SCRIPT_CONFIG_FILE_DST),
    )?;
    spinner.finish();

    logger::outro(MSG_ECOSYSTEM_TXN_OUTRO);

    Ok(())
}
