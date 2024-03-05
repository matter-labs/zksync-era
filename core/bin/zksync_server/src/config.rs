use anyhow::Context as _;
use zksync_core::{consensus, temp_config_store::decode_json};

pub(crate) fn read_consensus_secrets() -> anyhow::Result<Option<consensus::Secrets>> {
    // Read public config.
    let Ok(path) = std::env::var("CONSENSUS_SECRETS_PATH") else {
        return Ok(None);
    };
    let secrets = std::fs::read_to_string(&path).context(path)?;
    Ok(Some(decode_json(&secrets).context("failed decoding JSON")?))
}

pub(crate) fn read_consensus_config() -> anyhow::Result<Option<consensus::Config>> {
    // Read public config.
    let Ok(path) = std::env::var("CONSENSUS_CONFIG_PATH") else {
        return Ok(None);
    };
    let cfg = std::fs::read_to_string(&path).context(path)?;
    Ok(Some(decode_json(&cfg).context("failed decoding JSON")?))
}
