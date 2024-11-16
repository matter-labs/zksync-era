use anyhow::Context;
use common::{
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
};
use config::{
    forge_interface::{
        register_chain::input::RegisterChainL1Config, script_params::REGISTER_CHAIN_SCRIPT_PARAMS,
    },
    traits::SaveConfig,
    ContractsConfig, EcosystemConfig,
};
use ethers::{abi::Address, utils::hex::ToHexExt};
use xshell::Shell;
use zksync_basic_types::L2ChainId;

use crate::{
    commands::ecosystem::args::register_chain::RegisterChainArgs,
    messages::{
        MSG_CHAIN_REGISTERED, MSG_CHAIN_TRANSACTIONS_BUILT, MSG_CHAIN_TXN_OUT_PATH_INVALID_ERR,
        MSG_REGISTERING_CHAIN_SPINNER, MSG_WRITING_OUTPUT_FILES_SPINNER,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

pub const REGISTER_CHAIN_TXNS_FILE_SRC: &str =
    "contracts/l1-contracts/broadcast/RegisterHyperchain.s.sol/9/dry-run/run-latest.json";
pub const REGISTER_CHAIN_TXNS_FILE_DST: &str = "register-hyperchain-txns.json";
const SCRIPT_CONFIG_FILE_DST: &str = "register-hyperchain.toml";

pub async fn run(args: RegisterChainArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config.load_current_chain()?;
    let wallets = ecosystem_config.get_wallets()?;
    let final_args = args.fill_values_with_prompt(chain_config.chain_id, wallets.governor.address);
    let contracts = ecosystem_config.get_contracts_config()?;
    dbg!(&final_args);
    let l1_rpc_url = final_args.l1_rpc_url;
    let spinner = Spinner::new(MSG_REGISTERING_CHAIN_SPINNER);
    register_chain(
        shell,
        final_args.forge_script_args,
        &ecosystem_config,
        &contracts,
        final_args.chain_id,
        l1_rpc_url,
        None,
        final_args.proposal_author,
        !final_args.no_broadcast,
    )
    .await?;
    spinner.finish();
    logger::success(MSG_CHAIN_REGISTERED);

    if final_args.no_broadcast {
        let out = final_args.out;
        let spinner = Spinner::new(MSG_WRITING_OUTPUT_FILES_SPINNER);
        shell
            .create_dir(&out)
            .context(MSG_CHAIN_TXN_OUT_PATH_INVALID_ERR)?;

        shell.copy_file(
            ecosystem_config
                .link_to_code
                .join(REGISTER_CHAIN_TXNS_FILE_SRC),
            out.join(REGISTER_CHAIN_TXNS_FILE_DST),
        )?;

        shell.copy_file(
            REGISTER_CHAIN_SCRIPT_PARAMS.input(&ecosystem_config.link_to_code),
            out.join(SCRIPT_CONFIG_FILE_DST),
        )?;
        spinner.finish();

        logger::success(MSG_CHAIN_TRANSACTIONS_BUILT);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn register_chain(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    contracts: &ContractsConfig,
    chain_id: L2ChainId,
    l1_rpc_url: String,
    sender: Option<Address>,
    proposal_author: Address,
    broadcast: bool,
) -> anyhow::Result<()> {
    let deploy_config_path = REGISTER_CHAIN_SCRIPT_PARAMS.input(&config.link_to_code);

    let deploy_config = RegisterChainL1Config::new(chain_id, contracts, proposal_author)?;
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry())
        .script(&REGISTER_CHAIN_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url);

    if broadcast {
        forge = forge.with_broadcast();
    }

    if let Some(address) = sender {
        forge = forge.with_sender(address.encode_hex_upper());
    } else {
        forge = fill_forge_private_key(forge, Some(&config.get_wallets()?.governor))?;
        check_the_balance(&forge).await?;
    }

    forge.run(shell)?;
    Ok(())
}
