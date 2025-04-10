use anyhow::Context;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, config::global_config, logger};
use zkstack_cli_config::EcosystemConfig;

use crate::commands::dev::messages::MSG_CHAIN_NOT_FOUND_ERR;

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let general_config = chain_config.get_general_config().await?;

    let mut command = cmd!(
        shell,
        "cargo run --manifest-path ./core/Cargo.toml --release --bin loadnext"
    )
    .env(
        "L2_CHAIN_ID",
        chain_config
            .get_genesis_config()
            .await?
            .l2_chain_id()?
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
    .env("L2_RPC_ADDRESS", general_config.l2_http_url()?)
    .env("L2_WS_RPC_ADDRESS", general_config.l2_ws_url()?);

    if global_config().verbose {
        command = command.env("RUST_LOG", "loadnext=info")
    }

    Cmd::new(command).with_force_run().run()?;

    logger::outro("Loadtest success");

    Ok(())
}
