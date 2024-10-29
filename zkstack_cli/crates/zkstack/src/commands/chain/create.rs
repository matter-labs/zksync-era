use std::cell::OnceCell;

use anyhow::Context;
use common::logger;
use config::{
    create_local_configs_dir, create_wallets, get_default_era_chain_id,
    traits::{ReadConfigWithBasePath, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig, GenesisConfig, LOCAL_ARTIFACTS_PATH, LOCAL_DB_PATH,
};
use xshell::Shell;
use zksync_basic_types::L2ChainId;

use crate::{
    commands::chain::args::create::{ChainCreateArgs, ChainCreateArgsFinal},
    messages::{
        MSG_ARGS_VALIDATOR_ERR, MSG_CHAIN_CREATED, MSG_CREATING_CHAIN,
        MSG_EVM_EMULATOR_HASH_MISSING_ERR, MSG_SELECTED_CONFIG,
    },
    utils::link_to_code::resolve_link_to_code,
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

    let link_to_code = ecosystem
        .as_ref()
        .map(|ecosystem| ecosystem.link_to_code.clone().display().to_string());

    let args = args
        .fill_values_with_prompt(
            shell,
            number_of_chains,
            l1_network,
            possible_erc20,
            chains_path,
            link_to_code,
            era_chain_id,
        )
        .context(MSG_ARGS_VALIDATOR_ERR)?;

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&args));
    logger::info(MSG_CREATING_CHAIN);

    let name = args.chain_name.clone();
    let set_as_default = args.set_as_default;
    create_chain_inner(args, shell)?;
    if let Some(ecosystem) = ecosystem {
        if set_as_default {
            ecosystem.default_chain = name;
            ecosystem.save_with_base_path(shell, ".")?;
        }
    }

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
    let internal_id = args.number_of_chains;
    let link_to_code = resolve_link_to_code(shell, chain_path.clone(), args.link_to_code.clone())?;
    let default_genesis_config = GenesisConfig::read_with_base_path(
        shell,
        EcosystemConfig::default_configs_path(&link_to_code),
    )?;
    let has_evm_emulation_support = default_genesis_config.evm_emulator_hash.is_some();
    if args.evm_emulator && !has_evm_emulation_support {
        anyhow::bail!(MSG_EVM_EMULATOR_HASH_MISSING_ERR);
    }
    let rocks_db_path = chain_path.join(LOCAL_DB_PATH);
    let artifacts = chain_path.join(LOCAL_ARTIFACTS_PATH);

    let chain_config = ChainConfig {
        id: internal_id,
        name: default_chain_name.clone(),
        chain_id,
        prover_version: args.prover_version,
        l1_network: args.l1_network,
        link_to_code: link_to_code.clone(),
        rocks_db_path,
        artifacts,
        configs: chain_configs_path.clone(),
        external_node_config_path: None,
        l1_batch_commit_data_generator_mode: args.l1_batch_commit_data_generator_mode,
        base_token: args.base_token,
        wallet_creation: args.wallet_creation,
        shell: OnceCell::from(shell.clone()),
        legacy_bridge,
        evm_emulator: args.evm_emulator,
    };

    create_wallets(
        shell,
        &chain_config.configs,
        &link_to_code,
        internal_id,
        args.wallet_creation,
        args.wallet_path,
    )?;

    chain_config.save_with_base_path(shell, chain_path)?;
    Ok(())
}
