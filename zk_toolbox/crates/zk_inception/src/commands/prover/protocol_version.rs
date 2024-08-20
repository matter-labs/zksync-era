use std::{fs, path::PathBuf};

use common::logger;
use config::EcosystemConfig;
use xshell::{cmd, Shell};

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let link_to_code = EcosystemConfig::from_file(shell)?.link_to_code;

    let protocol_version = get_protocol_version(shell, &link_to_code).await?;
    let snark_wrapper = get_snark_wrapper(&link_to_code).await?;

    logger::info(format!(
        "Current protocol version found in zksync-era: {}, snark_wrapper: {}",
        protocol_version, snark_wrapper
    ));

    Ok(())
}

async fn get_protocol_version(shell: &Shell, link_to_code: &PathBuf) -> anyhow::Result<String> {
    let path = link_to_code.join("prover/crates/bin/prover_version/Cargo.toml");

    let protocol_version = cmd!(shell, "cargo run --manifest-path {path}").read()?;

    Ok(protocol_version)
}

async fn get_snark_wrapper(link_to_code: &PathBuf) -> anyhow::Result<String> {
    let path = link_to_code
        .join("prover/crates/bin/vk_setup_data_generator_server_fri/data/commitments.json");
    let file = fs::File::open(path).expect("Could not find commitments file in zksync-era");
    let json: serde_json::Value =
        serde_json::from_reader(file).expect("Could not parse commitments.json");

    let snark_wrapper = json
        .get("snark_wrapper")
        .expect("Could not find snark_wrapper in commitments.json");

    Ok(snark_wrapper.to_string())
}
