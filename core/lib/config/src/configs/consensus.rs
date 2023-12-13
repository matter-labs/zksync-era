use serde::de::Error;
use std::collections::{HashMap, HashSet};
use zksync_consensus_crypto::{Text, TextFmt};
use zksync_consensus_roles::{node, validator};

#[derive(PartialEq, Eq, Hash)]
pub struct SerdeText<T: TextFmt>(pub T);

impl<'de, T: TextFmt> serde::Deserialize<'de> for SerdeText<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self(
            T::decode(Text::new(<&str>::deserialize(d)?)).map_err(Error::custom)?,
        ))
    }
}

#[derive(serde::Deserialize)]
pub struct GossipConfig {
    /// Key of this node. It uniquely identifies the node.
    pub key: SerdeText<node::SecretKey>,
    /// Limit on the number of inbound connections outside
    /// of the `static_inbound` set.
    pub dynamic_inbound_limit: u64,
    /// Inbound connections that should be unconditionally accepted.
    pub static_inbound: HashSet<SerdeText<node::PublicKey>>,
    /// Outbound connections that the node should actively try to
    /// establish and maintain.
    pub static_outbound: HashMap<SerdeText<node::PublicKey>, std::net::SocketAddr>,
}

#[derive(serde::Deserialize)]
pub struct ValidatorConfig {
    key: SerdeText<validator::SecretKey>,
    public_addr: std::net::SocketAddr,
}

#[derive(serde::Deserialize)]
pub struct NodeConfig {
    server_addr: std::net::SocketAddr,
    gossip: GossipConfig,
    validator: Option<ValidatorConfig>,
    validator_set: Vec<SerdeText<validator::PublicKey>>,
}
