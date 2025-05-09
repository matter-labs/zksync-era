use std::collections::{BTreeMap, BTreeSet};

use secrecy::{ExposeSecret as _, SecretString};
use zksync_basic_types::{ethabi, L2ChainId};
use zksync_concurrency::{limiter, time};

/// `zksync_consensus_crypto::TextFmt` representation of `zksync_consensus_roles::validator::PublicKey`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValidatorPublicKey(pub String);

/// `zksync_consensus_crypto::TextFmt` representation of `zksync_consensus_roles::validator::SecretKey`.
#[derive(Debug, Clone)]
pub struct ValidatorSecretKey(pub SecretString);

/// `zksync_consensus_crypto::TextFmt` representation of `zksync_consensus_roles::node::PublicKey`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodePublicKey(pub String);

/// `zksync_consensus_crypto::TextFmt` representation of `zksync_consensus_roles::node::SecretKey`.
#[derive(Debug, Clone)]
pub struct NodeSecretKey(pub SecretString);

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
    /// Address of the registry contract.
    pub registry_address: Option<ethabi::Address>,
    /// Recommended list of peers to connect to.
    pub seed_peers: BTreeMap<NodePublicKey, Host>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct RpcConfig {
    /// Max number of blocks that can be send from/to each peer.
    /// Defaults to 10 blocks/s/connection.
    pub get_block_rate: Option<limiter::Rate>,
}

impl RpcConfig {
    pub fn get_block_rate(&self) -> limiter::Rate {
        self.get_block_rate.unwrap_or(limiter::Rate {
            burst: 10,
            refresh: time::Duration::milliseconds(100),
        })
    }
}

/// Config (shared between main node and external node).
#[derive(Clone, Debug, PartialEq)]
pub struct ConsensusConfig {
    pub port: Option<u16>,
    /// Local socket address to listen for the incoming connections.
    pub server_addr: std::net::SocketAddr,
    /// Public address of this node (should forward to `server_addr`)
    /// that will be advertised to peers, so that they can connect to this
    /// node.
    pub public_addr: Host,

    /// Maximal allowed size of the payload in bytes.
    pub max_payload_size: usize,

    /// View timeout duration in milliseconds.
    pub view_timeout: Option<time::Duration>,

    /// Maximal allowed size of the sync-batch payloads in bytes.
    ///
    /// The batch consists of block payloads and a Merkle proof of inclusion on L1 (~1kB),
    /// so the maximum batch size should be the maximum payload size times the maximum number
    /// of blocks in a batch.
    pub max_batch_size: usize,

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

    /// Rate limiting configuration for the p2p RPCs.
    pub rpc: Option<RpcConfig>,

    /// Local socket address to expose the node debug page.
    pub debug_page_addr: Option<std::net::SocketAddr>,
}

impl ConsensusConfig {
    pub fn view_timeout(&self) -> time::Duration {
        self.view_timeout.unwrap_or(time::Duration::seconds(2))
    }

    pub fn rpc(&self) -> RpcConfig {
        self.rpc.clone().unwrap_or_default()
    }
}

/// Secrets needed for consensus.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsensusSecrets {
    pub validator_key: Option<ValidatorSecretKey>,
    pub node_key: Option<NodeSecretKey>,
}
