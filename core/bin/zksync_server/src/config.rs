use anyhow::Context as _;
use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};

pub(crate) fn read_consensus_secrets() -> anyhow::Result<Option<ConsensusSecrets>> {
    // Read public config.
    let Ok(path) = std::env::var("CONSENSUS_SECRETS_PATH") else {
        return Ok(None);
    };
    Ok(Some(
        read_yaml_repr::<proto::secrets::ConsensusSecrets>(&path.into())
            .context("failed decoding YAML")?,
    ))
}

pub(crate) fn read_consensus_config() -> anyhow::Result<Option<ConsensusConfig>> {
    // Read public config.
    let Ok(path) = std::env::var("CONSENSUS_CONFIG_PATH") else {
        return Ok(None);
    };
    Ok(Some(
        read_yaml_repr::<proto::consensus::Config>(&path.into()).context("failed decoding YAML")?,
    ))
}
