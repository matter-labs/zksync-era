//! Configuration utilities for the consensus component.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context as _;
use zksync_consensus_crypto::{read_required_text, Text, TextFmt};
use zksync_consensus_executor as executor;
use zksync_consensus_roles::{node, validator};
use zksync_protobuf::{required, ProtoFmt};

use crate::consensus::proto;

/// Decodes a proto message from json for arbitrary `ProtoFmt`.
pub fn decode_json<T: ProtoFmt>(json: &str) -> anyhow::Result<T> {
    let mut d = serde_json::Deserializer::from_str(json);
    let p: T = zksync_protobuf::serde::deserialize(&mut d)?;
    d.end()?;
    Ok(p)
}

/// Decodes a secret of type T from an env var with name `var_name`.
/// It makes sure that the error message doesn't contain the secret.
pub fn read_secret<T: TextFmt>(var_name: &str) -> anyhow::Result<T> {
    let raw = std::env::var(var_name).map_err(|_| anyhow::anyhow!("{var_name} not set"))?;
    Text::new(&raw)
        .decode()
        .map_err(|_| anyhow::anyhow!("{var_name} has invalid format"))
}

/// Config (shared between main node and external node).
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Local socket address to listen for the incoming connections.
    pub server_addr: std::net::SocketAddr,
    /// Public address of this node (should forward to `server_addr`)
    /// that will be advertised to peers, so that they can connect to this
    /// node.
    pub public_addr: std::net::SocketAddr,

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
    pub gossip_static_outbound: BTreeMap<node::PublicKey, std::net::SocketAddr>,
}

impl Config {
    pub fn executor_config(&self, node_key: node::SecretKey) -> executor::Config {
        executor::Config {
            server_addr: self.server_addr,
            validators: self.validators.clone(),
            max_payload_size: self.max_payload_size,
            node_key,
            gossip_dynamic_inbound_limit: self.gossip_dynamic_inbound_limit,
            gossip_static_inbound: self.gossip_static_inbound.clone().into_iter().collect(),
            gossip_static_outbound: self.gossip_static_outbound.clone().into_iter().collect(),
        }
    }

    pub fn validator_config(
        &self,
        validator_key: validator::SecretKey,
    ) -> executor::ValidatorConfig {
        executor::ValidatorConfig {
            public_addr: self.public_addr,
            key: validator_key,
        }
    }
}

impl ProtoFmt for Config {
    type Proto = proto::ConsensusConfig;
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
            let addr = read_required_text(&e.addr)
                .with_context(|| format!("gossip_static_outbound[{i}].addr"))?;
            gossip_static_outbound.insert(key, addr);
        }
        Ok(Self {
            server_addr: read_required_text(&r.server_addr).context("server_addr")?,
            public_addr: read_required_text(&r.public_addr).context("public_addr")?,
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
            public_addr: Some(self.public_addr.encode()),
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
                    addr: Some(TextFmt::encode(addr)),
                })
                .collect(),
            gossip_dynamic_inbound_limit: Some(
                self.gossip_dynamic_inbound_limit.try_into().unwrap(),
            ),
        }
    }
}
