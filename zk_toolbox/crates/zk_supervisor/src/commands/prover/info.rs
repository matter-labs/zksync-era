use anyhow::Context as _;
use std::path::PathBuf;
use std::{fs, path::Path};

use crate::messages::MSG_CHAIN_NOT_FOUND_ERR;
use common::config::global_config;
use common::logger;
use config::EcosystemConfig;
use xshell::{cmd, Shell};

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let link_to_code = EcosystemConfig::from_file(shell)?.link_to_code;
    let link_to_prover = link_to_code.join("prover");

    let protocol_version = get_protocol_version(shell, &link_to_prover).await?;
    let snark_wrapper = get_snark_wrapper(&link_to_prover).await?;
    let prover_url = get_database_url(shell).await?;

    logger::info(format!(
        "
=============================== \
Current prover setup information: \
Protocol version: {} \
Snark wrapper: {} \
Database URL: {}\
===============================",
        protocol_version, snark_wrapper, prover_url
    ));

    Ok(())
}

pub(crate) async fn get_protocol_version(
    shell: &Shell,
    link_to_prover: &PathBuf,
) -> anyhow::Result<String> {
    shell.change_dir(link_to_prover);
    let protocol_version = cmd!(shell, "cargo run --release --bin prover_version").read()?;

    Ok(protocol_version)
}

pub(crate) async fn get_snark_wrapper(link_to_prover: &PathBuf) -> anyhow::Result<String> {
    let path = link_to_prover.join("data/keys/commitments.json");
    let file = fs::File::open(path).expect("Could not find commitments file in zksync-era");
    let json: serde_json::Value =
        serde_json::from_reader(file).expect("Could not parse commitments.json");

    let snark_wrapper = json
        .get("snark_wrapper")
        .expect("Could not find snark_wrapper in commitments.json");

    Ok(snark_wrapper.to_string())
}

pub(crate) async fn get_database_url(shell: &Shell) -> anyhow::Result<String> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem
        .load_chain(global_config().chain_name.clone())
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let prover_url = chain_config
        .get_secrets_config()?
        .database
        .context("Database secrets not found")?
        .prover_url()?
        .expose_url()
        .to_string();
    Ok(prover_url)
}
