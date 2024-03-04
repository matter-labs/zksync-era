use anyhow::Context as _;
use zksync_consensus_roles::{node, validator};
use zksync_core::consensus;

pub(crate) struct Secrets;

impl consensus::config::Secrets for Secrets {
    fn validator_key(&self) -> anyhow::Result<validator::SecretKey> {
        consensus::config::read_secret("CONSENSUS_VALIDATOR_KEY")
    }

    fn node_key(&self) -> anyhow::Result<node::SecretKey> {
        consensus::config::read_secret("CONSENSUS_NODE_KEY")
    }
}

pub(crate) fn read_consensus_config() -> anyhow::Result<Option<consensus::config::Config>> {
    // Read public config.
    let Ok(path) = std::env::var("CONSENSUS_CONFIG_PATH") else {
        return Ok(None);
    };
    let cfg = std::fs::read_to_string(&path).context(path)?;
    Ok(Some(
        consensus::config::decode_json(&cfg).context("failed decoding JSON")?,
    ))
}
