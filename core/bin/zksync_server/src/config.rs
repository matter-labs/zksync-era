use anyhow::Context as _;
use zksync_core::{consensus, temp_config_store::decode_yaml};

pub(crate) fn read_consensus_secrets() -> anyhow::Result<Option<consensus::Secrets>> {
    // Read public config.
    let Ok(path) = std::env::var("CONSENSUS_SECRETS_PATH") else {
        return Ok(None);
    };
    let secrets = std::fs::read_to_string(&path).context(path)?;
    Ok(Some(decode_yaml(&secrets).context("failed decoding YAML")?))
}

pub(crate) fn read_consensus_config() -> anyhow::Result<Option<consensus::Config>> {
    // Read public config.
    let Ok(path) = std::env::var("CONSENSUS_CONFIG_PATH") else {
        return Ok(None);
    };
    let cfg = std::fs::read_to_string(&path).context(path)?;
    Ok(Some(decode_yaml(&cfg).context("failed decoding YAML")?))
}
