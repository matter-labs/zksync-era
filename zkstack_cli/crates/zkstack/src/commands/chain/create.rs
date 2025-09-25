use std::cell::OnceCell;

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::config::global_config;
use zkstack_cli_common::{logger, spinner::Spinner};
use zkstack_cli_config::{
    create_local_configs_dir, create_wallets, traits::SaveConfigWithBasePath, ChainConfig,
    EcosystemConfig, GenesisConfig, ZkStackConfig, ZkStackConfigTrait, GENESIS_FILE,
};
use zksync_basic_types::L2ChainId;

use crate::{
    commands::chain::args::create::{ChainCreateArgs, ChainCreateArgsFinal},
    messages::{
        MSG_ARGS_VALIDATOR_ERR, MSG_CHAIN_CREATED, MSG_CREATING_CHAIN,
        MSG_CREATING_CHAIN_CONFIGURATIONS_SPINNER, MSG_EVM_EMULATOR_HASH_MISSING_ERR,
        MSG_SELECTED_CONFIG,
    },
};

pub async fn run(args: ChainCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    // TODO support creating without ecosystem
    let mut ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    create(args, &mut ecosystem_config, shell).await
}

pub async fn create(
    args: ChainCreateArgs,
    ecosystem_config: &mut EcosystemConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    let tokens = ecosystem_config.get_erc20_tokens();
    let args = args
        .fill_values_with_prompt(
            ecosystem_config.list_of_chains().len() as u32,
            &ecosystem_config.l1_network,
            tokens,
        )
        .context(MSG_ARGS_VALIDATOR_ERR)?;

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&args));
    logger::info(MSG_CREATING_CHAIN);

    let spinner = Spinner::new(MSG_CREATING_CHAIN_CONFIGURATIONS_SPINNER);
    let name = args.chain_name.clone();
    let set_as_default = args.set_as_default;
    create_chain_inner(args, ecosystem_config, shell).await?;
    if set_as_default {
        ecosystem_config.set_default_chain(name);
        ecosystem_config.save_with_base_path(shell, ".")?;
    }
    spinner.finish();

    logger::success(MSG_CHAIN_CREATED);

    Ok(())
}

pub(crate) async fn create_chain_inner(
    args: ChainCreateArgsFinal,
    ecosystem_config: &EcosystemConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    if args.legacy_bridge {
        logger::warn("WARNING!!! You are creating a chain with legacy bridge, use it only for testing compatibility")
    }
    let default_chain_name = args.chain_name.clone();
    println!(
        "ecosystem_config.list_of_chains() before: {:?}",
        ecosystem_config.list_of_chains()
    );
    let internal_id = if ecosystem_config.list_of_chains().contains(&args.chain_name) {
        ecosystem_config
            .list_of_chains()
            .iter()
            .position(|x| *x == args.chain_name)
            .unwrap() as u32
            + 1
    } else {
        ecosystem_config.list_of_chains().len() as u32 + 1
    };
    println!("internal_id: {}", internal_id);
    let chain_path = ecosystem_config.chains.join(&default_chain_name);
    let chain_configs_path = create_local_configs_dir(shell, &chain_path)?;
    let (chain_id, legacy_bridge) = if args.legacy_bridge {
        // Legacy bridge is distinguished by using the same chain id as ecosystem
        (ecosystem_config.era_chain_id, Some(true))
    } else {
        (L2ChainId::from(args.chain_id), None)
    };
    println!(
        "ecosystem_config.list_of_chains() after: {:?}",
        ecosystem_config.list_of_chains()
    );
    let genesis_config_path = ecosystem_config.default_configs_path().join(GENESIS_FILE);
    let default_genesis_config = GenesisConfig::read(shell, &genesis_config_path).await?;
    let has_evm_emulation_support = default_genesis_config.evm_emulator_hash()?.is_some();
    if args.evm_emulator && !has_evm_emulation_support {
        anyhow::bail!(MSG_EVM_EMULATOR_HASH_MISSING_ERR);
    }

    let chain_config = ChainConfig::new(
        internal_id,
        default_chain_name.clone(),
        chain_id,
        args.prover_version,
        ecosystem_config.l1_network,
        chain_path.clone(),
        ecosystem_config.link_to_code(),
        ecosystem_config.get_chain_rocks_db_path(&default_chain_name),
        ecosystem_config.get_chain_artifacts_path(&default_chain_name),
        chain_configs_path.clone(),
        None,
        args.l1_batch_commit_data_generator_mode,
        args.base_token,
        args.wallet_creation,
        OnceCell::from(shell.clone()),
        legacy_bridge,
        args.evm_emulator,
        args.tight_ports,
        global_config().zksync_os,
        Some(ecosystem_config.contracts_path()),
    );

    create_wallets(
        shell,
        &chain_config.configs,
        &ecosystem_config.link_to_code(),
        internal_id,
        args.wallet_creation,
        args.wallet_path,
    )?;

    chain_config.save_with_base_path(shell, chain_path)?;
    Ok(())
}
