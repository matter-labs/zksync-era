//! Configuration utilities for the consensus component.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context as _;
use zksync_concurrency::net;
use zksync_consensus_crypto::{read_required_text, Text, TextFmt};
use zksync_consensus_executor as executor;
use zksync_consensus_roles::{node, validator};
use zksync_protobuf::{required, ProtoFmt};

use crate::{
    consensus::{fetcher::P2PConfig, MainNodeConfig},
    proto::consensus as proto,
};

fn read_optional_secret_text<T: TextFmt>(text: &Option<String>) -> anyhow::Result<Option<T>> {
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

impl Config {
    pub fn main_node(&self, secrets: &Secrets) -> anyhow::Result<MainNodeConfig> {
        Ok(MainNodeConfig {
            executor: self.executor_config(secrets.node_key.clone().context("missing node_key")?),
            validator_key: secrets
                .validator_key
                .clone()
                .context("missing validator_key")?,
        })
    }

    pub fn p2p(&self, secrets: &Secrets) -> anyhow::Result<P2PConfig> {
        Ok(self.executor_config(secrets.node_key.clone().context("missing node_key")?))
    }

    fn executor_config(&self, node_key: node::SecretKey) -> executor::Config {
        executor::Config {
            server_addr: self.server_addr,
            public_addr: self.public_addr.clone(),
            max_payload_size: self.max_payload_size,
            node_key,
            gossip_dynamic_inbound_limit: self.gossip_dynamic_inbound_limit,
            gossip_static_inbound: self.gossip_static_inbound.clone().into_iter().collect(),
            gossip_static_outbound: self.gossip_static_outbound.clone().into_iter().collect(),
        }
    }
}

impl ProtoFmt for Config {
    type Proto = proto::Config;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        let validators = r
            .validators
            .iter()
            .enumerate()
            .map(|(i, v)| {
                Text::new(v)
                    .decode()
                    .with_context(|| format!("validators[{i}]"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let validators = validator::ValidatorSet::new(validators).context("validators")?;

        let mut gossip_static_inbound = BTreeSet::new();
        for (i, v) in r.gossip_static_inbound.iter().enumerate() {
            gossip_static_inbound.insert(
                Text::new(v)
                    .decode()
                    .with_context(|| format!("gossip_static_inbound[{i}]"))?,
            );
        }
        let mut gossip_static_outbound = BTreeMap::new();
        for (i, e) in r.gossip_static_outbound.iter().enumerate() {
            let key = read_required_text(&e.key)
                .with_context(|| format!("gossip_static_outbound[{i}].key"))?;
            let addr = net::Host(
                required(&e.addr)
                    .with_context(|| format!("gossip_static_outbound[{i}].addr"))?
                    .clone(),
            );
            gossip_static_outbound.insert(key, addr);
        }
        Ok(Self {
            server_addr: read_required_text(&r.server_addr).context("server_addr")?,
            public_addr: net::Host(required(&r.public_addr).context("public_addr")?.clone()),
            validators,
            max_payload_size: required(&r.max_payload_size)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_payload_size")?,
            gossip_dynamic_inbound_limit: required(&r.gossip_dynamic_inbound_limit)
                .and_then(|x| Ok((*x).try_into()?))
                .context("gossip_dynamic_inbound_limit")?,
            gossip_static_inbound,
            gossip_static_outbound,
        })
    }

    fn build(&self) -> Self::Proto {
        Self::Proto {
            server_addr: Some(self.server_addr.encode()),
            public_addr: Some(self.public_addr.0.clone()),
            validators: self.validators.iter().map(TextFmt::encode).collect(),
            max_payload_size: Some(self.max_payload_size.try_into().unwrap()),
            gossip_static_inbound: self
                .gossip_static_inbound
                .iter()
                .map(TextFmt::encode)
                .collect(),
            gossip_static_outbound: self
                .gossip_static_outbound
                .iter()
                .map(|(key, addr)| proto::NodeAddr {
                    key: Some(TextFmt::encode(key)),
                    addr: Some(addr.0.clone()),
                })
                .collect(),
            gossip_dynamic_inbound_limit: Some(
                self.gossip_dynamic_inbound_limit.try_into().unwrap(),
            ),
        }
    }
}

#[derive(Debug)]
pub struct Secrets {
    pub validator_key: Option<validator::SecretKey>,
    pub node_key: Option<node::SecretKey>,
}

impl ProtoFmt for Secrets {
    type Proto = proto::Secrets;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            validator_key: read_optional_secret_text(&r.validator_key).context("validator_key")?,
            node_key: read_optional_secret_text(&r.node_key).context("node_key")?,
        })
    }

    fn build(&self) -> Self::Proto {
        Self::Proto {
            validator_key: self.validator_key.as_ref().map(TextFmt::encode),
            node_key: self.node_key.as_ref().map(TextFmt::encode),
        }
    }
}
