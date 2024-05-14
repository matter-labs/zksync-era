//! Configuration utilities for the consensus component.
use std::collections::HashMap;

use anyhow::Context as _;
use zksync_concurrency::net;
use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets, Host, NodePublicKey};
use zksync_consensus_crypto::{Text, TextFmt};
use zksync_consensus_executor as executor;
use zksync_consensus_roles::{node, validator};
use zksync_types::L2ChainId;

use crate::consensus::{fetcher::P2PConfig, MainNodeConfig};

fn read_secret_text<T: TextFmt>(text: Option<&String>) -> anyhow::Result<T> {
    Text::new(text.context("missing")?)
        .decode()
        .map_err(|_| anyhow::format_err!("invalid format"))
}

fn validator_key(secrets: &ConsensusSecrets) -> anyhow::Result<validator::SecretKey> {
    read_secret_text(secrets.validator_key.as_ref().map(|x| &x.0))
}

fn node_key(secrets: &ConsensusSecrets) -> anyhow::Result<node::SecretKey> {
    read_secret_text(secrets.node_key.as_ref().map(|x| &x.0))
}

/// Constructs a main node config from raw config.
pub fn main_node(
    cfg: &ConsensusConfig,
    secrets: &ConsensusSecrets,
    chain_id: L2ChainId,
) -> anyhow::Result<MainNodeConfig> {
    Ok(MainNodeConfig {
        executor: executor(cfg, secrets)?,
        validator_key: validator_key(secrets).context("validator_key")?,
        chain_id: validator::ChainId(chain_id.as_u64()),
    })
}

pub(super) fn p2p(cfg: &ConsensusConfig, secrets: &ConsensusSecrets) -> anyhow::Result<P2PConfig> {
    executor(cfg, secrets)
}

fn executor(cfg: &ConsensusConfig, secrets: &ConsensusSecrets) -> anyhow::Result<executor::Config> {
    let mut gossip_static_outbound = HashMap::new();
    {
        let mut append = |key: &NodePublicKey, addr: &Host| {
            gossip_static_outbound.insert(
                Text::new(&key.0).decode().context("key")?,
                net::Host(addr.0.clone()),
            );
            anyhow::Ok(())
        };
        for (i, (k, v)) in cfg.gossip_static_outbound.iter().enumerate() {
            append(k, v).with_context(|| format!("gossip_static_outbound[{i}]"))?;
        }
    }
    Ok(executor::Config {
        server_addr: cfg.server_addr,
        public_addr: net::Host(cfg.public_addr.0.clone()),
        max_payload_size: cfg.max_payload_size,
        node_key: node_key(secrets).context("node_key")?,
        gossip_dynamic_inbound_limit: cfg.gossip_dynamic_inbound_limit,
        gossip_static_inbound: cfg
            .gossip_static_inbound
            .iter()
            .enumerate()
            .map(|(i, x)| Text::new(&x.0).decode().context(i))
            .collect::<Result<_, _>>()
            .context("gossip_static_inbound")?,
        gossip_static_outbound,
    })
}
