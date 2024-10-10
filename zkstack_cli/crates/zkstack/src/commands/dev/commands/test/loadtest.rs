use anyhow::Context;
use common::{cmd::Cmd, config::global_config, logger};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::commands::dev::messages::MSG_CHAIN_NOT_FOUND_ERR;

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let general_api = chain_config
        .get_general_config()?
        .api_config
        .context("API config is not found")?;

    let mut command = cmd!(shell, "cargo run --release --bin loadnext")
        .env(
            "L2_CHAIN_ID",
            chain_config
                .get_genesis_config()?
                .l2_chain_id
                .as_u64()
                .to_string(),
        )
        .env(
            "MAIN_TOKEN",
            format!(
                "{:?}",
                ecosystem_config
                    .get_erc20_tokens()
                    .first()
                    .context("NO Erc20 tokens were deployed")?
                    .address
            ),
        )
        .env("L2_RPC_ADDRESS", general_api.web3_json_rpc.http_url)
        .env("L2_WS_RPC_ADDRESS", general_api.web3_json_rpc.ws_url);

    if global_config().verbose {
        command = command.env("RUST_LOG", "loadnext=info")
    }

    Cmd::new(command).with_force_run().run()?;

    logger::outro("Loadtest success");

    Ok(())
}
