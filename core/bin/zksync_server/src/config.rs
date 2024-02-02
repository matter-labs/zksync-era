use anyhow::Context as _;
use zksync_consensus_roles::{node, validator};
use zksync_core::consensus;

pub(crate) fn read_consensus_config() -> anyhow::Result<consensus::MainNodeConfig> {
    let path = std::env::var("CONSENSUS_CONFIG_PATH").context("CONSENSUS_CONFIG_PATH")?;
    let cfg = std::fs::read_to_string(&path).context(path)?;
    let cfg: consensus::config::Config =
        consensus::config::decode_json(&cfg).context("failed decoding JSON")?;
    let validator_key: validator::SecretKey =
        consensus::config::read_secret("CONSENSUS_VALIDATOR_KEY")?;
    let node_key: node::SecretKey = consensus::config::read_secret("CONSENSUS_NODE_KEY")?;
    Ok(consensus::MainNodeConfig {
        executor: cfg.executor_config(node_key),
        validator: cfg.validator_config(validator_key),
    })
}
