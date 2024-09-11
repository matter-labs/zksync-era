use config::ChainConfig;
use secrecy::Secret;
use zksync_config::configs::consensus::{
    AttesterPublicKey, AttesterSecretKey, ConsensusSecrets, GenesisSpec, NodeSecretKey,
    ProtocolVersion, ValidatorPublicKey, ValidatorSecretKey, WeightedAttester, WeightedValidator,
};
use zksync_consensus_crypto::TextFmt;
use zksync_consensus_roles as roles;

struct ConsensusKeys {
    validator_key: roles::validator::SecretKey,
    attester_key: roles::attester::SecretKey,
    node_key: roles::node::SecretKey,
}

struct ConsensusPublicKeys {
    validator_key: roles::validator::PublicKey,
    attester_key: roles::attester::PublicKey,
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
