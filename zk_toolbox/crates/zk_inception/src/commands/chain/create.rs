use std::cell::OnceCell;

use common::{logger, spinner::Spinner};
use xshell::Shell;

use crate::{
    commands::chain::args::create::{ChainCreateArgs, ChainCreateArgsFinal},
    configs::{ChainConfig, EcosystemConfig, SaveConfig},
    consts::{CONFIG_NAME, LOCAL_CONFIGS_PATH, LOCAL_DB_PATH, WALLETS_FILE},
    messages::{
        MSG_CHAIN_CREATED, MSG_CREATING_CHAIN, MSG_CREATING_CHAIN_CONFIGURATIONS_SPINNER,
        MSG_SELECTED_CONFIG,
    },
    types::ChainId,
    wallets::create_wallets,
};

pub fn run(args: ChainCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    let mut ecosystem_config = EcosystemConfig::from_file(shell)?;
    create(args, &mut ecosystem_config, shell)
}

fn create(
    args: ChainCreateArgs,
    ecosystem_config: &mut EcosystemConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt(ecosystem_config.list_of_chains().len() as u32);

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&args));
    logger::info(MSG_CREATING_CHAIN);

    let spinner = Spinner::new(MSG_CREATING_CHAIN_CONFIGURATIONS_SPINNER);
    let name = args.chain_name.clone();
    let set_as_default = args.set_as_default;
    create_chain_inner(args, ecosystem_config, shell)?;
    if set_as_default {
        ecosystem_config.default_chain = name;
        ecosystem_config.save(shell, CONFIG_NAME)?;
    }
    spinner.finish();

    logger::success(MSG_CHAIN_CREATED);

    Ok(())
}

pub(crate) fn create_chain_inner(
    args: ChainCreateArgsFinal,
    ecosystem_config: &EcosystemConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    let default_chain_name = args.chain_name.clone();
    let chain_path = ecosystem_config.chains.join(&default_chain_name);
    let chain_configs_path = shell.create_dir(chain_path.join(LOCAL_CONFIGS_PATH))?;
    let chain_db_path = chain_path.join(LOCAL_DB_PATH);
    let chain_id = ecosystem_config.list_of_chains().len() as u32;

    let chain_config = ChainConfig {
        id: chain_id,
        name: default_chain_name.clone(),
        chain_id: ChainId::from(args.chain_id),
        prover_version: args.prover_version,
        l1_network: ecosystem_config.l1_network,
        link_to_code: ecosystem_config.link_to_code.clone(),
        rocks_db_path: chain_db_path,
        configs: chain_configs_path.clone(),
        l1_batch_commit_data_generator_mode: args.l1_batch_commit_data_generator_mode,
        base_token: args.base_token,
        wallet_creation: args.wallet_creation,
        shell: OnceCell::from(shell.clone()),
    };

    create_wallets(
        shell,
        &chain_config.configs.join(WALLETS_FILE),
        &ecosystem_config.link_to_code,
        chain_id,
        args.wallet_creation,
        args.wallet_path,
    )?;

    chain_config.save(shell, chain_path.join(CONFIG_NAME))?;
    Ok(())
}
