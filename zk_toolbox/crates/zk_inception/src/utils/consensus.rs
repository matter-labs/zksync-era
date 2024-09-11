use std::collections::{BTreeMap, BTreeSet};

use config::{ChainConfig, PortsConfig};
use secrecy::{ExposeSecret, Secret};
use zksync_config::configs::consensus::{
    AttesterPublicKey, AttesterSecretKey, ConsensusConfig, ConsensusSecrets, GenesisSpec, Host,
    NodePublicKey, NodeSecretKey, ProtocolVersion, ValidatorPublicKey, ValidatorSecretKey,
    WeightedAttester, WeightedValidator,
};
use zksync_consensus_crypto::{Text, TextFmt};
use zksync_consensus_roles as roles;

use crate::consts::{
    CONSENSUS_PUBLIC_ADDRESS_HOST, CONSENSUS_SERVER_ADDRESS_HOST, GOSSIP_DYNAMIC_INBOUND_LIMIT,
    MAX_BATCH_SIZE, MAX_PAYLOAD_SIZE,
};

#[derive(Debug, Clone)]
struct ConsensusKeys {
    validator_key: roles::validator::SecretKey,
    attester_key: roles::attester::SecretKey,
    node_key: roles::node::SecretKey,
}

struct ConsensusPublicKeys {
    validator_key: roles::validator::PublicKey,
    attester_key: roles::attester::PublicKey,
}

pub fn get_consensus_config(
    chain_config: &ChainConfig,
    ports: PortsConfig,
    consensus_keys: Option<ConsensusKeys>,
    gossip_static_outbound: Option<BTreeMap<NodePublicKey, Host>>,
) -> anyhow::Result<ConsensusConfig> {
    let genesis_spec = if let Some(consensus_keys) = consensus_keys {
        Some(get_genesis_specs(chain_config, &consensus_keys))
    } else {
        None
    };

    let public_addr = format!("{}:{}", CONSENSUS_PUBLIC_ADDRESS_HOST, ports.consensus_port);
    let server_addr =
        format!("{}:{}", CONSENSUS_SERVER_ADDRESS_HOST, ports.consensus_port).parse()?;

    Ok(ConsensusConfig {
        server_addr,
        public_addr: Host(public_addr),
        genesis_spec,
        max_payload_size: MAX_PAYLOAD_SIZE,
        gossip_dynamic_inbound_limit: GOSSIP_DYNAMIC_INBOUND_LIMIT,
        max_batch_size: MAX_BATCH_SIZE,
        gossip_static_inbound: BTreeSet::new(),
        gossip_static_outbound: gossip_static_outbound.unwrap_or_default(),
        rpc: None,
    })
}

pub fn generate_consensus_keys() -> ConsensusKeys {
    ConsensusKeys {
        validator_key: roles::validator::SecretKey::generate(),
        attester_key: roles::attester::SecretKey::generate(),
        node_key: roles::node::SecretKey::generate(),
    }
}

fn get_consensus_public_keys(consensus_keys: &ConsensusKeys) -> ConsensusPublicKeys {
    ConsensusPublicKeys {
        validator_key: consensus_keys.validator_key.public(),
        attester_key: consensus_keys.attester_key.public(),
    }
}

pub fn get_genesis_specs(
    chain_config: &ChainConfig,
    consensus_keys: &ConsensusKeys,
) -> GenesisSpec {
    let public_keys = get_consensus_public_keys(consensus_keys);
    let validator_key = public_keys.validator_key.encode();
    let attester_key = public_keys.attester_key.encode();

    let validator = WeightedValidator {
        key: ValidatorPublicKey(validator_key.clone()),
        weight: 1,
    };
    let attester = WeightedAttester {
        key: AttesterPublicKey(attester_key),
        weight: 1,
    };
    let leader = ValidatorPublicKey(validator_key);

    GenesisSpec {
        chain_id: chain_config.chain_id,
        protocol_version: ProtocolVersion(1),
        validators: vec![validator],
        attesters: vec![attester],
        leader,
        registry_address: None,
    }
}

pub fn get_consensus_secrets(consensus_keys: &ConsensusKeys) -> ConsensusSecrets {
    let validator_key = consensus_keys.validator_key.encode();
    let attester_key = consensus_keys.attester_key.encode();
    let node_key = consensus_keys.node_key.encode();

    ConsensusSecrets {
        validator_key: Some(ValidatorSecretKey(Secret::new(validator_key))),
        attester_key: Some(AttesterSecretKey(Secret::new(attester_key))),
        node_key: Some(NodeSecretKey(Secret::new(node_key))),
    }
}

pub fn node_public_key(secrets: &ConsensusSecrets) -> anyhow::Result<Option<NodePublicKey>> {
    Ok(node_key(secrets)?.map(|node_secret_key| NodePublicKey(node_secret_key.public().encode())))
}
fn node_key(secrets: &ConsensusSecrets) -> anyhow::Result<Option<roles::node::SecretKey>> {
    read_secret_text(secrets.node_key.as_ref().map(|x| &x.0))
}

fn read_secret_text<T: TextFmt>(text: Option<&Secret<String>>) -> anyhow::Result<Option<T>> {
    text.map(|text| Text::new(text.expose_secret()).decode())
        .transpose()
        .map_err(|_| anyhow::format_err!("invalid format"))
}
