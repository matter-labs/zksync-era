use anyhow::Context;
use ethers::utils::hex::ToHexExt;
use xshell::Shell;
use zkstack_cli_common::{logger, spinner::Spinner};
use zkstack_cli_config::{
    copy_configs, traits::SaveConfigWithBasePath, ZkStackConfig, ZkStackConfigTrait,
};

use crate::{
    admin_functions::unpause_deposits,
    commands::chain::{
        args::build_transactions::BuildTransactionsArgs, register_chain::register_chain,
    },
    messages::{
        MSG_BUILDING_CHAIN_REGISTRATION_TXNS_SPINNER, MSG_CHAIN_NOT_FOUND_ERR,
        MSG_CHAIN_TRANSACTIONS_BUILT, MSG_CHAIN_TXN_MISSING_CONTRACT_CONFIG,
        MSG_CHAIN_TXN_OUT_PATH_INVALID_ERR, MSG_PREPARING_CONFIG_SPINNER, MSG_SELECTED_CONFIG,
        MSG_UNPAUSING_DEPOSITS_SPINNER, MSG_WRITING_OUTPUT_FILES_SPINNER,
    },
};

pub const REGISTER_CHAIN_TXNS_FILE_SRC: &str =
    "l1-contracts/broadcast/RegisterZKChain.s.sol/9/dry-run/run-latest.json";
pub const REGISTER_CHAIN_TXNS_FILE_DST: &str = "register-zk-chain-txns.json";

const SCRIPT_CONFIG_FILE_SRC: &str = "l1-contracts/script-config/register-zk-chain.toml";
const SCRIPT_CONFIG_FILE_DST: &str = "register-zk-chain.toml";

pub(crate) async fn run(args: BuildTransactionsArgs, shell: &Shell) -> anyhow::Result<()> {
    let config = ZkStackConfig::ecosystem(shell)?;
    let chain_config = config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let vm_option = chain_config.vm_option;

    let args = args.fill_values_with_prompt(chain_config.name.clone());

    let spinner = Spinner::new(MSG_PREPARING_CONFIG_SPINNER);
    copy_configs(
        shell,
        &config.default_configs_path_for_ctm(vm_option),
        &chain_config.configs,
    )?;

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&chain_config));

    let mut genesis_config = chain_config.get_genesis_config().await?.patched();
    genesis_config.update_from_chain_config(&chain_config)?;
    genesis_config.save().await?;

    // Copy ecosystem contracts
    let contracts_config = config
        .get_contracts_config()
        .context(MSG_CHAIN_TXN_MISSING_CONTRACT_CONFIG)?;
    spinner.finish();

    let spinner = Spinner::new(MSG_BUILDING_CHAIN_REGISTRATION_TXNS_SPINNER);
    let governor: String = config.get_wallets()?.governor.address.encode_hex_upper();

    let mut contracts = register_chain(
        shell,
        args.forge_args.clone(),
        &config,
        &chain_config,
        &contracts_config,
        args.l1_rpc_url.clone(),
        Some(governor),
        false,
    )
    .await?;

    contracts.l1.base_token_addr = chain_config.base_token.address;
    contracts.save_with_base_path(shell, &args.out)?;
    spinner.finish();

    if !args.pause_deposits {
        // Deposits are paused by default to allow immediate Gateway migration, so we need to unpause them.
        let spinner = Spinner::new(MSG_UNPAUSING_DEPOSITS_SPINNER);
        unpause_deposits(
            shell,
            &args.forge_args,
            &chain_config.path_to_foundry_scripts(),
            crate::admin_functions::AdminScriptMode::Broadcast(
                chain_config.get_wallets_config()?.governor,
            ),
            chain_config.chain_id.as_u64(),
            contracts_config
                .core_ecosystem_contracts
                .bridgehub_proxy_addr,
            args.l1_rpc_url.clone(),
        )
        .await?;
        spinner.finish();
    }

    let spinner = Spinner::new(MSG_WRITING_OUTPUT_FILES_SPINNER);
    shell
        .create_dir(&args.out)
        .context(MSG_CHAIN_TXN_OUT_PATH_INVALID_ERR)?;

    shell.copy_file(
        config
            .contracts_path_for_ctm(vm_option)
            .join(REGISTER_CHAIN_TXNS_FILE_SRC),
        args.out.join(REGISTER_CHAIN_TXNS_FILE_DST),
    )?;

    shell.copy_file(
        config
            .contracts_path_for_ctm(vm_option)
            .join(SCRIPT_CONFIG_FILE_SRC),
        args.out.join(SCRIPT_CONFIG_FILE_DST),
    )?;
    spinner.finish();

    logger::success(MSG_CHAIN_TRANSACTIONS_BUILT);
    Ok(())
}
