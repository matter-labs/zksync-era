use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use zksync_consensus_crypto::{read_required_text, Text, TextFmt};
use zksync_consensus_executor as executor;
use zksync_consensus_roles::{node, validator};
use zksync_core::consensus;
use zksync_protobuf::{required, ProtoFmt};
use zksync_types::Address;

use crate::proto;

/// Decodes a proto message from json for arbitrary ProtoFmt.
fn decode_json<T: ProtoFmt>(json: &str) -> anyhow::Result<T> {
    let mut d = serde_json::Deserializer::from_str(json);
    let p: T = zksync_protobuf::serde::deserialize(&mut d)?;
    d.end()?;
    Ok(p)
}

/// Decodes a secret of type T from an env var with name var_name.
/// It makes sure that the error message doesn't contain the secret.
fn read_secret<T: TextFmt>(var_name: &str) -> anyhow::Result<T> {
    let raw = std::env::var(var_name).map_err(|_| anyhow::anyhow!("{var_name} not set"))?;
    Text::new(&raw)
        .decode()
        .map_err(|_| anyhow::anyhow!("{var_name} has invalid format"))
}

pub(crate) fn read_consensus_config(
    operator_address: Address,
) -> anyhow::Result<Option<consensus::MainNodeConfig>> {
    let Ok(path) = std::env::var("CONSENSUS_CONFIG_PATH") else {
        return Ok(None);
    };
    let cfg = std::fs::read_to_string(&path).context(path)?;
    let cfg: ConsensusConfig = decode_json(&cfg).context("failed decoding JSON")?;
    let validator_key: validator::SecretKey = read_secret("CONSENSUS_VALIDATOR_KEY")?;
    let node_key: node::SecretKey = read_secret("CONSENSUS_NODE_KEY")?;
    let validators =
        validator::ValidatorSet::new([validator_key.public()]).context("validatorSet::new()")?;
    Ok(Some(consensus::MainNodeConfig {
        executor: executor::Config {
            server_addr: cfg.server_addr,
            validators,
            node_key,
            gossip_dynamic_inbound_limit: cfg.gossip_dynamic_inbound_limit,
            gossip_static_inbound: cfg.gossip_static_inbound,
            gossip_static_outbound: HashMap::new(),
        },
        validator: executor::ValidatorConfig {
            key: validator_key,
            public_addr: cfg.public_addr,
        },
        operator_address,
    }))
}

struct ConsensusConfig {
    pub server_addr: std::net::SocketAddr,
    pub public_addr: std::net::SocketAddr,
    pub gossip_static_inbound: HashSet<node::PublicKey>,
    pub gossip_dynamic_inbound_limit: u64,
}

impl ProtoFmt for ConsensusConfig {
    type Proto = proto::ConsensusConfig;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        let mut gossip_static_inbound = HashSet::new();
        for (i, v) in r.gossip_static_inbound.iter().enumerate() {
            gossip_static_inbound.insert(
                Text::new(v)
                    .decode()
                    .with_context(|| format!("gossip_static_inbound[{i}]"))?,
            );
        }
        Ok(Self {
            server_addr: read_required_text(&r.server_addr).context("server_addr")?,
            public_addr: read_required_text(&r.public_addr).context("public_addr")?,
            gossip_static_inbound,
            gossip_dynamic_inbound_limit: *required(&r.gossip_dynamic_inbound_limit)
                .context("gossip_dynamic_inbound_limit")?,
        })
    }

    fn build(&self) -> Self::Proto {
        Self::Proto {
            server_addr: Some(self.server_addr.encode()),
            public_addr: Some(self.public_addr.encode()),
            gossip_static_inbound: self
                .gossip_static_inbound
                .iter()
                .map(TextFmt::encode)
                .collect(),
            gossip_dynamic_inbound_limit: Some(self.gossip_dynamic_inbound_limit),
        }
    }
}
