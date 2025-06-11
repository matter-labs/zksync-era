use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroUsize,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use smart_config::{
    de,
    de::{DeserializeContext, Entries, Qualified, Serde, WellKnown},
    metadata::{BasicTypes, ParamMetadata, SizeUnit, TypeDescription},
    value::SecretString,
    ByteSize, DescribeConfig, DeserializeConfig, ErrorWithOrigin,
};
use zksync_basic_types::{ethabi, L2ChainId};
use zksync_concurrency::{limiter, time};

use crate::utils::Fallback;

/// `zksync_consensus_crypto::TextFmt` representation of `zksync_consensus_roles::validator::PublicKey`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ValidatorPublicKey(pub String);

impl WellKnown for ValidatorPublicKey {
    type Deserializer = Qualified<Serde![str]>;
    const DE: Self::Deserializer =
        Qualified::new(Serde![str], "has `validator:public:bls12_381:` prefix");
}

/// `zksync_consensus_crypto::TextFmt` representation of `zksync_consensus_roles::node::PublicKey`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodePublicKey(pub String);

impl WellKnown for NodePublicKey {
    type Deserializer = Qualified<Serde![str]>;
    const DE: Self::Deserializer = Qualified::new(Serde![str], "has `node:public:ed25519:` prefix");
}

/// Copy-paste of `zksync_concurrency::net::Host`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Host(pub String);

impl WellKnown for Host {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

/// Copy-paste of `zksync_consensus_roles::validator::ProtocolVersion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProtocolVersion(pub u32);

impl WellKnown for ProtocolVersion {
    type Deserializer = Serde![int];
    const DE: Self::Deserializer = Serde![int];
}

/// Consensus genesis specification.
/// It is a digest of the `validator::Genesis`,
/// which allows to initialize genesis (if not present) or
/// decide whether a hard fork is necessary (if present).
#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct GenesisSpec {
    /// Chain ID.
    #[config(with = Serde![int])]
    pub chain_id: L2ChainId,
    /// Consensus protocol version.
    pub protocol_version: ProtocolVersion,
    /// The validator committee. Represents `zksync_consensus_roles::validator::Committee`.
    #[config(default, with = Entries::WELL_KNOWN.named("key", "weight"))]
    pub validators: Vec<(ValidatorPublicKey, u64)>,
    /// Leader of the committee.
    pub leader: Option<ValidatorPublicKey>,
    /// Address of the registry contract.
    pub registry_address: Option<ethabi::Address>,
    /// Recommended list of peers to connect to.
    #[config(default, with = Entries::WELL_KNOWN.named("key", "addr"))]
    pub seed_peers: BTreeMap<NodePublicKey, Host>,
}

#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct RpcConfig {
    // FIXME: breaking change from `get_block_rate: Rate`, but it looks unused. (no mentions in configs)
    /// Max number of blocks that can be sent from/to each peer. Defaults to 10 blocks/s/connection.
    #[config(default_t = NonZeroUsize::new(10).unwrap())]
    pub get_block_rps: NonZeroUsize,
}

impl RpcConfig {
    pub fn get_block_rate(&self) -> limiter::Rate {
        let rps = self.get_block_rps.get();
        limiter::Rate {
            burst: rps,
            refresh: time::Duration::seconds(1) / (rps as f32),
        }
    }
}

// We cannot deserialize `Duration` directly because it expects an object with the `secs` (not `seconds`!) and `nanos` fields.
#[derive(Debug, Serialize, Deserialize)]
struct SerdeDuration {
    seconds: u64,
    nanos: u32,
}

#[derive(Debug)]
struct CustomDurationFormat;

impl de::DeserializeParam<Duration> for CustomDurationFormat {
    const EXPECTING: BasicTypes = BasicTypes::OBJECT;

    fn describe(&self, description: &mut TypeDescription) {
        description.set_details("object with `seconds` and `nanos` fields");
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Duration, ErrorWithOrigin> {
        let duration = SerdeDuration::deserialize(ctx.current_value_deserializer(param.name)?)?;
        Ok(Duration::new(duration.seconds, duration.nanos))
    }

    fn serialize_param(&self, param: &Duration) -> serde_json::Value {
        let duration = SerdeDuration {
            seconds: param.as_secs(),
            nanos: param.subsec_nanos(),
        };
        serde_json::to_value(duration).unwrap()
    }
}

/// Config (shared between main node and external node).
#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ConsensusConfig {
    pub port: Option<u16>,
    /// Local socket address to listen for the incoming connections.
    pub server_addr: std::net::SocketAddr,
    /// Public address of this node (should forward to `server_addr`)
    /// that will be advertised to peers, so that they can connect to this
    /// node.
    pub public_addr: Host,
    /// Maximal allowed size of the payload in bytes.
    #[config(default_t = ByteSize(2_500_000), with = Fallback(SizeUnit::Bytes))]
    pub max_payload_size: ByteSize,
    /// View timeout duration.
    #[config(default_t = Duration::from_secs(2), with = Fallback(CustomDurationFormat))]
    pub view_timeout: Duration,
    /// Maximal allowed size of the sync-batch payloads in bytes.
    ///
    /// The batch consists of block payloads and a Merkle proof of inclusion on L1 (~1kB),
    /// so the maximum batch size should be the maximum payload size times the maximum number
    /// of blocks in a batch.
    #[config(default_t = ByteSize(12_500_001_024), with = Fallback(SizeUnit::Bytes))]
    pub max_batch_size: ByteSize,

    /// Limit on the number of inbound connections outside the `static_inbound` set.
    #[config(default_t = 100)]
    pub gossip_dynamic_inbound_limit: usize,
    /// Inbound gossip connections that should be unconditionally accepted.
    #[config(default)]
    pub gossip_static_inbound: BTreeSet<NodePublicKey>,
    /// Outbound gossip connections that the node should actively try to
    /// establish and maintain.
    #[config(default, with = Entries::WELL_KNOWN.named("key", "addr"))]
    pub gossip_static_outbound: BTreeMap<NodePublicKey, Host>,

    /// MAIN NODE ONLY: consensus genesis specification.
    /// Used to (re)initialize genesis if needed.
    /// External nodes fetch the genesis from the main node.
    #[config(nest)]
    pub genesis_spec: Option<GenesisSpec>,

    /// Rate limiting configuration for the p2p RPCs.
    #[config(nest)]
    pub rpc: RpcConfig,

    /// Local socket address to expose the node debug page.
    pub debug_page_addr: Option<std::net::SocketAddr>,
}

impl ConsensusConfig {
    pub fn rpc(&self) -> RpcConfig {
        self.rpc.clone()
    }
}

/// Secrets needed for consensus.
#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ConsensusSecrets {
    /// Has `validator:secret:bls12_381:` prefix.
    pub validator_key: Option<SecretString>,
    /// Has `node:secret:ed25519:` prefix.
    pub node_key: Option<SecretString>,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Yaml};

    use super::*;

    fn expected_config() -> ConsensusConfig {
        const VALIDATOR: &str = "validator:public:bls12_381:80ee14e3a66e1324b9af2531054a823a1224b433\
            6c6e46fa9491220bb94c887a66b14549060046537e17f831453d358d0b0662322437876622a2490246c0fe0a\
            6a0b5013062d50a6411ddb745189da7e5d79ffd64903e899c8b5a3e7cd89be5a";

        ConsensusConfig {
            port: Some(2954),
            server_addr: "127.0.0.1:2954".parse().unwrap(),
            public_addr: Host("127.0.0.1:2954".into()),
            max_payload_size: ByteSize(2000000),
            view_timeout: Duration::from_secs(3),
            max_batch_size: ByteSize(125001024),
            gossip_dynamic_inbound_limit: 10,
            gossip_static_inbound: BTreeSet::from([
                NodePublicKey("node:public:ed25519:5c270ee08cae1179a65845a62564ae5d216cbe2c97ed5083f512f2df353bb291".into())
            ]),
            gossip_static_outbound: BTreeMap::from([(
                NodePublicKey("node:public:ed25519:5c270ee08cae1179a65845a62564ae5d216cbe2c97ed5083f512f2df353bb291".into()),
                Host("127.0.0.1:3054".into())
            )]),
            genesis_spec: Some(GenesisSpec {
                chain_id: L2ChainId::from(271),
                protocol_version: ProtocolVersion(1),
                validators: vec![(ValidatorPublicKey(VALIDATOR.into()), 1)],
                leader: Some(ValidatorPublicKey(VALIDATOR.into())),
                registry_address: Some("0x2469b58c37e02d53a65cdf248d4086beba17de85".parse().unwrap()),
                seed_peers: BTreeMap::from([(
                    NodePublicKey("node:public:ed25519:68d29127ab03408bf5c838553b19c32bdb3aaaae9bf293e5e078c3a0d265822a".into()),
                    Host("example.com:3054".into())
                )]),
            }),
            rpc: RpcConfig {
                get_block_rps: NonZeroUsize::new(5).unwrap(),
            },
            debug_page_addr: Some("127.0.0.1:3000".parse().unwrap()),
        }
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
            server_addr: 127.0.0.1:2954
            public_addr: 127.0.0.1:2954
            debug_page_addr: 127.0.0.1:3000
            max_payload_size: 2000000
            view_timeout:
              seconds: 3
              nanos: 0
            gossip_dynamic_inbound_limit: 10
            gossip_static_inbound:
            - node:public:ed25519:5c270ee08cae1179a65845a62564ae5d216cbe2c97ed5083f512f2df353bb291
            gossip_static_outbound:
            - key: node:public:ed25519:5c270ee08cae1179a65845a62564ae5d216cbe2c97ed5083f512f2df353bb291
              addr: 127.0.0.1:3054
            genesis_spec:
              chain_id: 271
              protocol_version: 1
              validators:
              - key: validator:public:bls12_381:80ee14e3a66e1324b9af2531054a823a1224b4336c6e46fa9491220bb94c887a66b14549060046537e17f831453d358d0b0662322437876622a2490246c0fe0a6a0b5013062d50a6411ddb745189da7e5d79ffd64903e899c8b5a3e7cd89be5a
                weight: 1
              leader: validator:public:bls12_381:80ee14e3a66e1324b9af2531054a823a1224b4336c6e46fa9491220bb94c887a66b14549060046537e17f831453d358d0b0662322437876622a2490246c0fe0a6a0b5013062d50a6411ddb745189da7e5d79ffd64903e899c8b5a3e7cd89be5a
              seed_peers:
              - key: 'node:public:ed25519:68d29127ab03408bf5c838553b19c32bdb3aaaae9bf293e5e078c3a0d265822a'
                addr: 'example.com:3054'
              registry_address: 0x2469b58c37e02d53a65cdf248d4086beba17de85
            max_batch_size: 125001024
            port: 2954
            rpc:
              get_block_rps: 5
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ConsensusConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_idiomatic_yaml() {
        let yaml = r#"
            server_addr: 127.0.0.1:2954
            public_addr: 127.0.0.1:2954
            debug_page_addr: 127.0.0.1:3000
            max_payload_size: 2000000 bytes
            view_timeout: 3s
            gossip_dynamic_inbound_limit: 10
            gossip_static_inbound:
            - node:public:ed25519:5c270ee08cae1179a65845a62564ae5d216cbe2c97ed5083f512f2df353bb291
            gossip_static_outbound:
             'node:public:ed25519:5c270ee08cae1179a65845a62564ae5d216cbe2c97ed5083f512f2df353bb291': 127.0.0.1:3054
            genesis_spec:
              chain_id: 271
              protocol_version: 1
              validators:
                'validator:public:bls12_381:80ee14e3a66e1324b9af2531054a823a1224b4336c6e46fa9491220bb94c887a66b14549060046537e17f831453d358d0b0662322437876622a2490246c0fe0a6a0b5013062d50a6411ddb745189da7e5d79ffd64903e899c8b5a3e7cd89be5a': 1
              leader: validator:public:bls12_381:80ee14e3a66e1324b9af2531054a823a1224b4336c6e46fa9491220bb94c887a66b14549060046537e17f831453d358d0b0662322437876622a2490246c0fe0a6a0b5013062d50a6411ddb745189da7e5d79ffd64903e899c8b5a3e7cd89be5a
              seed_peers:
                'node:public:ed25519:68d29127ab03408bf5c838553b19c32bdb3aaaae9bf293e5e078c3a0d265822a': 'example.com:3054'
              registry_address: 0x2469b58c37e02d53a65cdf248d4086beba17de85
            max_batch_size: 125001024 bytes
            port: 2954
            rpc:
              get_block_rps: 5
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ConsensusConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
