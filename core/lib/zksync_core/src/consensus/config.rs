//! Configuration utilities for the consensus component.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context as _;
use zksync_concurrency::net;
use zksync_consensus_crypto::{read_required_text, Text, TextFmt};
use zksync_consensus_executor as executor;
use zksync_consensus_roles::{node, validator};
use zksync_protobuf::{required, ProtoFmt};
use zksync_config::configs::consensus::{ConsensusSecrets,ConsensusConfig};

use crate::{
    consensus::{fetcher::P2PConfig, MainNodeConfig},
    proto::consensus as proto,
};

fn read_secret_text<T: TextFmt>(text: &Option<String>) -> anyhow::Result<Option<T>> {
    text.as_ref()
        .map(|t| Text::new(t).decode())
        .transpose()
        .map_err(|_| anyhow::format_err!("invalid format"))
}

/// Config (shared between main node and external node).
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Local socket address to listen for the incoming connections.
    pub server_addr: std::net::SocketAddr,
    /// Public address of this node (should forward to `server_addr`)
    /// that will be advertised to peers, so that they can connect to this
    /// node.
    pub public_addr: net::Host,

    /// Validators participating in consensus.
    pub validators: validator::ValidatorSet,

    /// Maximal allowed size of the payload in bytes.
    pub max_payload_size: usize,

    /// Limit on the number of inbound connections outside
    /// of the `static_inbound` set.
    pub gossip_dynamic_inbound_limit: usize,
    /// Inbound gossip connections that should be unconditionally accepted.
    pub gossip_static_inbound: BTreeSet<node::PublicKey>,
    /// Outbound gossip connections that the node should actively try to
    /// establish and maintain.
    pub gossip_static_outbound: BTreeMap<node::PublicKey, net::Host>,
}

fn validator_key(secrets: &ConsensusSecrets) -> anyhow::Result<validator::SecretKey> {
    read_secret_text(&secrets.validator_key.as_ref().context("missing")?.0)
}

fn node_key(secrets: &ConsensusSecrets) -> anyhow::Result<node::SecretKey> {
    read_secret_text(&secrets.node_key.as_ref().context("missing")?.0)
}

pub(super) fn main_node(cfg: &ConsensusConfig, secrets: &ConsensusSecrets) -> anyhow::Result<MainNodeConfig> {
    Ok(MainNodeConfig {
        executor: executor(cfg,node_key(secrets).context("node_key")?)?,
        validator_key: validator_key(secrets).context("validator_key")?,
    })
}

pub(super) fn p2p(cfg: &ConsensusConfig, secrets: &ConsensusSecrets) -> anyhow::Result<P2PConfig> {
    cfg.executor_config(node_key(secrets).context("node_key")?)
}

fn executor(cfg: &ConsensusConfig, node_key: node::SecretKey) -> anyhow::Result<executor::Config> {
    executor::Config {
        server_addr: cfg.server_addr,
        public_addr: net::Host(cfg.public_addr.0.clone()),
        max_payload_size: cfg.max_payload_size,
        node_key,
        gossip_dynamic_inbound_limit: cfg.gossip_dynamic_inbound_limit,
        gossip_static_inbound: cfg.gossip_static_inbound.iter().enumerate().map(|(i,x)|Text::new(&x.0).decode().context(i)).collect::<Result<_,_>>().context("gossip_static_inbound")?,
        gossip_static_outbound: cfg.gossip_static_outbound.iter().enumerate()
            .map(|(i,x)|Ok((Text::new(&x.0.0).decode().context("key").context(i)?, net::Host(x.1.0.clone()))))
            .collect::<Result<_,_>>().context("gossip_static_outbound")?,
    }
}
