use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use url::Url;
use xshell::{cmd, Shell};
use zkstack_cli_common::logger;
use zkstack_cli_config::{ChainConfig, ZkStackConfig};

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;

    let link_to_prover = chain_config.link_to_code.join("prover");

    let protocol_version = get_protocol_version(shell, &link_to_prover).await?;
    let snark_wrapper = get_snark_wrapper(&link_to_prover).await?;
    let fflonk_snark_wrapper = get_fflonk_snark_wrapper(&link_to_prover).await?;
    let prover_url = get_database_url(&chain_config).await?;

    logger::info(format!(
        "
=============================== \n
Current prover setup information: \n
Protocol version: {} \n
Snark wrapper: {} \n\
FFLONK Snark wrapper: {} \n
Database URL: {}\n
===============================",
        protocol_version, snark_wrapper, fflonk_snark_wrapper, prover_url
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

pub(crate) async fn get_snark_wrapper(link_to_prover: &Path) -> anyhow::Result<String> {
    let path = link_to_prover.join("data/keys/commitments.json");
    let file = fs::File::open(path).expect("Could not find commitments file in zksync-era");
    let json: serde_json::Value =
        serde_json::from_reader(file).expect("Could not parse commitments.json");

    let snark_wrapper = json
        .get("snark_wrapper")
        .expect("Could not find snark_wrapper in commitments.json");

    let mut snark_wrapper = snark_wrapper.to_string();
    snark_wrapper.pop();
    snark_wrapper.remove(0);

    Ok(snark_wrapper)
}

pub(crate) async fn get_fflonk_snark_wrapper(link_to_prover: &Path) -> anyhow::Result<String> {
    let path = link_to_prover.join("data/keys/commitments.json");
    let file = fs::File::open(path).expect("Could not find commitments file in zksync-era");
    let json: serde_json::Value =
        serde_json::from_reader(file).expect("Could not parse commitments.json");

    let snark_wrapper = json
        .get("fflonk_snark_wrapper")
        .expect("Could not find snark_wrapper in commitments.json");

    let mut snark_wrapper = snark_wrapper.to_string();
    snark_wrapper.pop();
    snark_wrapper.remove(0);

    Ok(snark_wrapper)
}

pub(crate) async fn get_database_url(chain: &ChainConfig) -> anyhow::Result<Url> {
    chain
        .get_secrets_config()
        .await?
        .prover_database_url()?
        .context("missing prover database URL")
}

pub fn parse_version(version: &str) -> anyhow::Result<(&str, &str)> {
    let splitted: Vec<&str> = version.split(".").collect();

    assert_eq!(splitted.len(), 3, "Invalid version format");
    assert_eq!(splitted[0], "0", "Invalid major version, expected 0");

    splitted[1]
        .parse::<u32>()
        .context("Could not parse minor version")?;
    splitted[2]
        .parse::<u32>()
        .context("Could not parse patch version")?;

    let minor = splitted[1];
    let patch = splitted[2];

    Ok((minor, patch))
}
