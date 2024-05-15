use std::collections::{BTreeMap, BTreeSet};

use secrecy::{ExposeSecret as _, Secret};
use zksync_basic_types::L2ChainId;

/// `zksync_consensus_crypto::TextFmt` representation of `zksync_consensus_roles::validator::PublicKey`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValidatorPublicKey(pub String);

/// `zksync_consensus_crypto::TextFmt` representation of `zksync_consensus_roles::validator::SecretKey`.
#[derive(Debug, Clone)]
pub struct ValidatorSecretKey(pub Secret<String>);

/// `zksync_consensus_crypto::TextFmt` representation of `zksync_consensus_roles::node::PublicKey`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodePublicKey(pub String);

/// `zksync_consensus_crypto::TextFmt` representation of `zksync_consensus_roles::node::SecretKey`.
#[derive(Debug, Clone)]
pub struct NodeSecretKey(pub Secret<String>);

impl PartialEq for ValidatorSecretKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret().eq(other.0.expose_secret())
    }
}

impl PartialEq for NodeSecretKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret().eq(other.0.expose_secret())
    }
}

/// Copy-paste of `zksync_consensus_roles::validator::WeightedValidator`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightedValidator {
    /// Validator key
    pub key: ValidatorPublicKey,
    /// Validator weight inside the Committee.
    pub weight: u64,
}

/// Copy-paste of `zksync_concurrency::net::Host`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Host(pub String);

/// Copy-paste of `zksync_consensus_roles::validator::ProtocolVersion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProtocolVersion(pub u32);

/// Consensus genesis specification.
/// It is a digest of the `validator::Genesis`,
/// which allows to initialize genesis (if not present)
/// decide whether a hard fork is necessary (if present).
#[derive(Clone, Debug, PartialEq)]
pub struct GenesisSpec {
    /// Chain ID.
    pub chain_id: L2ChainId,
    /// Consensus protocol version.
    pub protocol_version: ProtocolVersion,
    /// The validator committee. Represents `zksync_consensus_roles::validator::Committee`.
    pub validators: Vec<WeightedValidator>,
    /// Leader of the committee. Represents
    /// `zksync_consensus_roles::validator::LeaderSelectionMode::Sticky`.
    pub leader: ValidatorPublicKey,
}

/// Config (shared between main node and external node).
#[derive(Clone, Debug, PartialEq)]
pub struct ConsensusConfig {
    /// Local socket address to listen for the incoming connections.
    pub server_addr: std::net::SocketAddr,
    /// Public address of this node (should forward to `server_addr`)
    /// that will be advertised to peers, so that they can connect to this
    /// node.
    pub public_addr: Host,

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

    /// MAIN NODE ONLY: consensus genesis specification.
    /// Used to (re)initialize genesis if needed.
    /// External nodes fetch the genesis from the main node.
    pub genesis_spec: Option<GenesisSpec>,
}

/// Secrets need for consensus.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsensusSecrets {
    pub validator_key: Option<ValidatorSecretKey>,
    pub node_key: Option<NodeSecretKey>,
}
