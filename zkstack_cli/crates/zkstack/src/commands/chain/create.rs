use std::cell::OnceCell;

use anyhow::Context;
use common::{logger, spinner::Spinner};
use config::{
    create_local_configs_dir, create_wallets, get_default_era_chain_id,
    traits::SaveConfigWithBasePath, ChainConfig, EcosystemConfig,
};
use xshell::Shell;
use zksync_basic_types::L2ChainId;

use crate::{
    commands::chain::args::create::{ChainCreateArgs, ChainCreateArgsFinal},
    messages::{
        MSG_ARGS_VALIDATOR_ERR, MSG_CHAIN_CREATED, MSG_CREATING_CHAIN,
        MSG_CREATING_CHAIN_CONFIGURATIONS_SPINNER, MSG_SELECTED_CONFIG,
    },
};

pub fn run(args: ChainCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    let mut ecosystem_config = EcosystemConfig::from_file(shell).ok();
    create(args, &mut ecosystem_config, shell)
}

fn create(
    args: ChainCreateArgs,
    ecosystem: &mut Option<EcosystemConfig>,
    shell: &Shell,
) -> anyhow::Result<()> {
    let possible_erc20 = ecosystem
        .as_ref()
        .map(|ecosystem| ecosystem.get_erc20_tokens())
        .unwrap_or(vec![]);

    let number_of_chains = ecosystem
        .as_ref()
        .map(|ecosystem| ecosystem.list_of_chains().len() as u32)
        .unwrap_or(0);

    let l1_network = ecosystem
        .as_ref()
        .map(|ecosystem| ecosystem.l1_network.clone());

    let chains_path = ecosystem.as_ref().map(|ecosystem| ecosystem.chains.clone());
    let era_chain_id = ecosystem
        .as_ref()
        .map(|ecosystem| ecosystem.era_chain_id)
        .unwrap_or(get_default_era_chain_id());

    let args = args
        .fill_values_with_prompt(
            number_of_chains,
            l1_network,
            possible_erc20,
            chains_path,
            era_chain_id,
        )
        .context(MSG_ARGS_VALIDATOR_ERR)?;

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&args));
    logger::info(MSG_CREATING_CHAIN);

    let spinner = Spinner::new(MSG_CREATING_CHAIN_CONFIGURATIONS_SPINNER);
    let name = args.chain_name.clone();
    let set_as_default = args.set_as_default;
    create_chain_inner(args, shell)?;
    if set_as_default {
        ecosystem_config.default_chain = name;
        ecosystem_config.save_with_base_path(shell, ".")?;
    }
    spinner.finish();

    logger::success(MSG_CHAIN_CREATED);

    Ok(())
}

pub(crate) fn create_chain_inner(args: ChainCreateArgsFinal, shell: &Shell) -> anyhow::Result<()> {
    if args.legacy_bridge {
        logger::warn("WARNING!!! You are creating a chain with legacy bridge, use it only for testing compatibility")
    }
    let default_chain_name = args.chain_name.clone();
    let chain_path = args.chain_path;
    let chain_configs_path = create_local_configs_dir(shell, &chain_path)?;
    let (chain_id, legacy_bridge) = if args.legacy_bridge {
        // Legacy bridge is distinguished by using the same chain id as ecosystem
        (args.era_chain_id, Some(true))
    } else {
        (L2ChainId::from(args.chain_id), None)
    };
    let internal_id = ecosystem_config.list_of_chains().len() as u32;

    let chain_config = ChainConfig {
        id: internal_id,
        name: default_chain_name.clone(),
        chain_id,
        prover_version: args.prover_version,
        l1_network: ecosystem_config.l1_network,
        link_to_code: ecosystem_config.link_to_code.clone(),
        rocks_db_path: ecosystem_config.get_chain_rocks_db_path(&default_chain_name),
        artifacts: ecosystem_config.get_chain_artifacts_path(&default_chain_name),
        configs: chain_configs_path.clone(),
        external_node_config_path: None,
        l1_batch_commit_data_generator_mode: args.l1_batch_commit_data_generator_mode,
        base_token: args.base_token,
        wallet_creation: args.wallet_creation,
        shell: OnceCell::from(shell.clone()),
        legacy_bridge,
    };

    create_wallets(
        shell,
        &chain_config.configs,
        &ecosystem_config.link_to_code,
        internal_id,
        args.wallet_creation,
        args.wallet_path,
    )?;

    chain_config.save_with_base_path(shell, chain_path)?;
    Ok(())
}
