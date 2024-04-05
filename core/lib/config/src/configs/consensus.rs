use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

/// Public key of the validator (consensus participant) of the form "validator:public:<signature scheme>:<hex encoded key material>"
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValidatorPublicKey(pub String);

// Secret key of the validator (consensus participant) of the form "validator:secret:<signature scheme>:<hex encoded key material>"
#[derive(PartialEq)]
pub struct ValidatorSecretKey(pub String);

/// Public key of the node (gossip network participant) of the form "node:public:<signature scheme>:<hex encoded key material>"
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodePublicKey(pub String);

// Secret key of the node (gossip network participant) of the form "node:secret:<signature scheme>:<hex encoded key material>"
#[derive(PartialEq)]
pub struct NodeSecretKey(pub String);

impl fmt::Debug for ValidatorSecretKey {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str("<redacted>")
    }
}

impl fmt::Debug for NodeSecretKey {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str("<redacted>")
    }
}

/// Network address in the `<domain/ip>:port` format.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Host(pub String);

/// Config (shared between main node and external node).
#[derive(Clone, Debug, PartialEq)]
pub struct ConsensusConfig {
    /// Local socket address to listen for the incoming connections.
    pub server_addr: std::net::SocketAddr,
    /// Public address of this node (should forward to `server_addr`)
    /// that will be advertised to peers, so that they can connect to this
    /// node.
    pub public_addr: Host,

    /// Validators participating in consensus.
    pub validators: BTreeSet<ValidatorPublicKey>,

    /// Maximal allowed size of the payload in bytes.
    pub max_payload_size: usize,

    /// Limit on the number of inbound connections outside
    /// of the `static_inbound` set.
    pub gossip_dynamic_inbound_limit: usize,
    /// Inbound gossip connections that should be unconditionally accepted.
    pub gossip_static_inbound: BTreeSet<NodePublicKey>,
    /// Outbound gossip connections that the node should actively try to
    /// establish and maintain.
    pub gossip_static_outbound: BTreeMap<NodePublicKey, Host>,
}

/// Secrets need for consensus.
#[derive(Debug, PartialEq)]
pub struct ConsensusSecrets {
    pub validator_key: Option<ValidatorSecretKey>,
    pub node_key: Option<NodeSecretKey>,
}
