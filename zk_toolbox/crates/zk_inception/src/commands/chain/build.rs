use anyhow::Context;
use common::{config::global_config, forge::Forge, git, logger, spinner::Spinner};
use config::{
    copy_configs,
    forge_interface::{
        register_chain::{input::RegisterChainL1Config, output::RegisterChainOutput},
        script_params::REGISTER_CHAIN_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    EcosystemConfig,
};
use ethers::utils::hex::ToHex;
use xshell::Shell;

use crate::{
    commands::chain::args::build::ChainBuildArgs,
    messages::{
        MSG_CHAIN_BUILD_MISSING_CONTRACT_CONFIG, MSG_CHAIN_BUILD_OUT_PATH_INVALID_ERR,
        MSG_CHAIN_INITIALIZED, MSG_CHAIN_NOT_FOUND_ERR, MSG_REGISTERING_CHAIN_SPINNER,
        MSG_SELECTED_CONFIG,
    },
};

const CHAIN_TRANSACTIONS_FILE: &str =
    "contracts/l1-contracts/broadcast/RegisterHyperchain.s.sol/9/dry-run/run-latest.json";
const SCRIPT_CONFIG_FILE: &str = "contracts/l1-contracts/script-config/register-hyperchain.toml";

pub(crate) async fn run(args: ChainBuildArgs, shell: &Shell) -> anyhow::Result<()> {
    let config = EcosystemConfig::from_file(shell)?;
    let chain_name = global_config().chain_name.clone();
    let chain_config = config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let args = args.fill_values_with_prompt(&chain_config);

    copy_configs(shell, &config.link_to_code, &chain_config.configs)?;

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&chain_config));

    git::submodule_update(shell, config.link_to_code.clone())?;

    // Copy ecosystem contracts
    let mut contracts_config = config
        .get_contracts_config()
        .context(MSG_CHAIN_BUILD_MISSING_CONTRACT_CONFIG)?;
    contracts_config.l1.base_token_addr = chain_config.base_token.address;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    let spinner = Spinner::new(MSG_REGISTERING_CHAIN_SPINNER);
    let deploy_config_path = REGISTER_CHAIN_SCRIPT_PARAMS.input(&config.link_to_code);

    let deploy_config = RegisterChainL1Config::new(&chain_config, &contracts_config)?;
    deploy_config.save(shell, deploy_config_path)?;

    Forge::new(&config.path_to_foundry())
        .script(
            &REGISTER_CHAIN_SCRIPT_PARAMS.script(),
            args.forge_args.clone(),
        )
        .with_ffi()
        .with_sender(config.get_wallets()?.governor.address.encode_hex_upper())
        .with_rpc_url(args.l1_rpc_url.clone())
        .run(shell)?;

    let register_chain_output = RegisterChainOutput::read(
        shell,
        REGISTER_CHAIN_SCRIPT_PARAMS.output(&chain_config.link_to_code),
    )?;
    contracts_config.set_chain_contracts(&register_chain_output);

    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    shell
        .create_dir(&args.out)
        .context(MSG_CHAIN_BUILD_OUT_PATH_INVALID_ERR)?;

    shell.copy_file(
        config.link_to_code.join(CHAIN_TRANSACTIONS_FILE),
        args.out.join("chain-txns.json"),
    )?;

    shell.copy_file(
        config.link_to_code.join(SCRIPT_CONFIG_FILE),
        args.out.join("chain-config.toml"),
    )?;
    spinner.finish();

    logger::success(MSG_CHAIN_INITIALIZED);
    Ok(())
}
