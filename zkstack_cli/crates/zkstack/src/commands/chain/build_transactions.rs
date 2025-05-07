use anyhow::Context;
use ethers::utils::hex::ToHex;
use xshell::Shell;
use zkstack_cli_common::{git, logger, spinner::Spinner};
use zkstack_cli_config::{copy_configs, traits::SaveConfigWithBasePath, EcosystemConfig};

use crate::{
    commands::chain::{
        args::build_transactions::BuildTransactionsArgs, register_chain::register_chain,
    },
    messages::{
        MSG_BUILDING_CHAIN_REGISTRATION_TXNS_SPINNER, MSG_CHAIN_NOT_FOUND_ERR,
        MSG_CHAIN_TRANSACTIONS_BUILT, MSG_CHAIN_TXN_MISSING_CONTRACT_CONFIG,
        MSG_CHAIN_TXN_OUT_PATH_INVALID_ERR, MSG_PREPARING_CONFIG_SPINNER, MSG_SELECTED_CONFIG,
        MSG_WRITING_OUTPUT_FILES_SPINNER,
    },
};

pub const REGISTER_CHAIN_TXNS_FILE_SRC: &str =
    "contracts/l1-contracts/broadcast/RegisterZKChain.s.sol/9/dry-run/run-latest.json";
pub const REGISTER_CHAIN_TXNS_FILE_DST: &str = "register-zk-chain-txns.json";

const SCRIPT_CONFIG_FILE_SRC: &str = "contracts/l1-contracts/script-config/register-zk-chain.toml";
const SCRIPT_CONFIG_FILE_DST: &str = "register-zk-chain.toml";

pub(crate) async fn run(args: BuildTransactionsArgs, shell: &Shell) -> anyhow::Result<()> {
    let config = EcosystemConfig::from_file(shell)?;
    let chain_config = config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let args = args.fill_values_with_prompt(config.default_chain.clone());

    git::submodule_update(shell, config.link_to_code.clone())?;

    let spinner = Spinner::new(MSG_PREPARING_CONFIG_SPINNER);
    copy_configs(shell, &config.link_to_code, &chain_config.configs)?;

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&chain_config));

    let mut genesis_config = chain_config.get_genesis_config().await?.patched();
    genesis_config.update_from_chain_config(&chain_config)?;
    // FIXME: config isn't saved; why?

    // Copy ecosystem contracts
    let mut contracts_config = config
        .get_contracts_config()
        .context(MSG_CHAIN_TXN_MISSING_CONTRACT_CONFIG)?;
    contracts_config.l1.base_token_addr = chain_config.base_token.address;
    spinner.finish();

    let spinner = Spinner::new(MSG_BUILDING_CHAIN_REGISTRATION_TXNS_SPINNER);
    let governor: String = config.get_wallets()?.governor.address.encode_hex_upper();

    register_chain(
        shell,
        args.forge_args.clone(),
        &config,
        &chain_config,
        &mut contracts_config,
        args.l1_rpc_url.clone(),
        Some(governor),
        false,
    )
    .await?;

    contracts_config.save_with_base_path(shell, &args.out)?;
    spinner.finish();

    let spinner = Spinner::new(MSG_WRITING_OUTPUT_FILES_SPINNER);
    shell
        .create_dir(&args.out)
        .context(MSG_CHAIN_TXN_OUT_PATH_INVALID_ERR)?;

    shell.copy_file(
        config.link_to_code.join(REGISTER_CHAIN_TXNS_FILE_SRC),
        args.out.join(REGISTER_CHAIN_TXNS_FILE_DST),
    )?;

    shell.copy_file(
        config.link_to_code.join(SCRIPT_CONFIG_FILE_SRC),
        args.out.join(SCRIPT_CONFIG_FILE_DST),
    )?;
    spinner.finish();

    logger::success(MSG_CHAIN_TRANSACTIONS_BUILT);
    Ok(())
}
