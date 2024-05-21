use std::cell::OnceCell;

use common::{logger, spinner::Spinner};
use xshell::Shell;

use crate::{
    commands::hyperchain::args::create::{HyperchainCreateArgs, HyperchainCreateArgsFinal},
    configs::{EcosystemConfig, HyperchainConfig, SaveConfig},
    consts::{CONFIG_NAME, LOCAL_CONFIGS_PATH, LOCAL_DB_PATH, WALLETS_FILE},
    types::ChainId,
    wallets::create_wallets,
};

pub fn run(args: HyperchainCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    let mut ecosystem_config = EcosystemConfig::from_file(shell)?;
    create(args, &mut ecosystem_config, shell)
}

fn create(
    args: HyperchainCreateArgs,
    ecosystem_config: &mut EcosystemConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt(ecosystem_config.list_of_hyperchains().len() as u32);

    logger::note("Selected config:", logger::object_to_string(&args));
    logger::info("Creating hyperchain");

    let spinner = Spinner::new("Creating hyperchain configurations...");
    let name = args.hyperchain_name.clone();
    let set_as_default = args.set_as_default;
    create_hyperchain_inner(args, ecosystem_config, shell)?;
    if set_as_default {
        ecosystem_config.default_hyperchain = name;
        ecosystem_config.save(shell, CONFIG_NAME)?;
    }
    spinner.finish();

    logger::success("Hyperchain created successfully");

    Ok(())
}

pub(crate) fn create_hyperchain_inner(
    args: HyperchainCreateArgsFinal,
    ecosystem_config: &EcosystemConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    let default_hyperchain_name = args.hyperchain_name.clone();
    let hyperchain_path = ecosystem_config.hyperchains.join(&default_hyperchain_name);
    let hyperchain_configs_path = shell.create_dir(hyperchain_path.join(LOCAL_CONFIGS_PATH))?;
    let hyperchain_db_path = hyperchain_path.join(LOCAL_DB_PATH);
    let hyperchain_id = ecosystem_config.list_of_hyperchains().len() as u32;

    let hyperchain_config = HyperchainConfig {
        id: hyperchain_id,
        name: default_hyperchain_name.clone(),
        chain_id: ChainId::from(args.chain_id),
        prover_version: args.prover_version,
        l1_network: ecosystem_config.l1_network,
        link_to_code: ecosystem_config.link_to_code.clone(),
        rocks_db_path: hyperchain_db_path,
        configs: hyperchain_configs_path.clone(),
        l1_batch_commit_data_generator_mode: args.l1_batch_commit_data_generator_mode,
        base_token: args.base_token,
        wallet_creation: args.wallet_creation,
        shell: OnceCell::from(shell.clone()),
    };

    create_wallets(
        shell,
        &hyperchain_config.configs.join(WALLETS_FILE),
        &ecosystem_config.link_to_code,
        hyperchain_id,
        args.wallet_creation,
        args.wallet_path,
    )?;

    hyperchain_config.save(shell, hyperchain_path.join(CONFIG_NAME))?;
    Ok(())
}
