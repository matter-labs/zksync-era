use anyhow::Context as _;
use common::yaml::ConfigPatch;
use config::ChainConfig;
use secrecy::{ExposeSecret, Secret};
use serde::Serialize;
use zksync_config::configs::consensus::{ConsensusSecrets, NodePublicKey, WeightedAttester};
use zksync_consensus_crypto::{Text, TextFmt};
use zksync_consensus_roles::{attester, node, validator};

// FIXME: remove
pub(crate) fn parse_attester_committee(
    attesters: &[WeightedAttester],
) -> anyhow::Result<attester::Committee> {
    let attesters: Vec<_> = attesters
        .iter()
        .enumerate()
        .map(|(i, v)| {
            Ok(attester::WeightedAttester {
                key: Text::new(&v.key.0).decode().context("key").context(i)?,
                weight: v.weight,
            })
        })
        .collect::<anyhow::Result<_>>()
        .context("attesters")?;
    attester::Committee::new(attesters).context("Committee::new()")
}

#[derive(Debug, Clone)]
pub struct ConsensusSecretKeys {
    validator_key: validator::SecretKey,
    attester_key: attester::SecretKey,
    node_key: node::SecretKey,
}

pub struct ConsensusPublicKeys {
    validator_key: validator::PublicKey,
    attester_key: attester::PublicKey,
}

pub fn generate_consensus_keys() -> ConsensusSecretKeys {
    ConsensusSecretKeys {
        validator_key: validator::SecretKey::generate(),
        attester_key: attester::SecretKey::generate(),
        node_key: node::SecretKey::generate(),
    }
}

fn get_consensus_public_keys(consensus_keys: &ConsensusSecretKeys) -> ConsensusPublicKeys {
    ConsensusPublicKeys {
        validator_key: consensus_keys.validator_key.public(),
        attester_key: consensus_keys.attester_key.public(),
    }
}

#[derive(Debug, Serialize)]
struct Weighted {
    key: String,
    weight: u64,
}

impl Weighted {
    fn new(key: String, weight: u64) -> Self {
        Self { key, weight }
    }
}

pub fn set_genesis_specs(
    general: &mut ConfigPatch,
    chain_config: &ChainConfig,
    consensus_keys: &ConsensusSecretKeys,
) {
    let public_keys = get_consensus_public_keys(consensus_keys);
    let validator_key = public_keys.validator_key.encode();
    let attester_key = public_keys.attester_key.encode();
    let leader = validator_key.clone();

    general.insert(
        "consensus.genesis_spec.chain_id",
        chain_config.chain_id.as_u64(),
    );
    general.insert("consensus.genesis_spec.protocol_version", 1_u64);
    general.insert_yaml(
        "consensus.genesis_spec.validators",
        [Weighted::new(validator_key, 1)],
    );
    general.insert_yaml(
        "consensus.genesis_spec.attesters",
        [Weighted::new(attester_key, 1)],
    );
    general.insert("consensus.genesis_spec.leader", leader);
}

pub fn set_consensus_secrets(secrets: &mut ConfigPatch, consensus_keys: &ConsensusSecretKeys) {
    let validator_key = consensus_keys.validator_key.encode();
    let attester_key = consensus_keys.attester_key.encode();
    let node_key = consensus_keys.node_key.encode();
    secrets.insert("consensus.validator_key", validator_key);
    secrets.insert("consensus.attester_key", attester_key);
    secrets.insert("consensus.node_key", node_key);
}

pub fn node_public_key(secrets: &ConsensusSecrets) -> anyhow::Result<Option<NodePublicKey>> {
    Ok(node_key(secrets)?.map(|node_secret_key| NodePublicKey(node_secret_key.public().encode())))
}

fn node_key(secrets: &ConsensusSecrets) -> anyhow::Result<Option<node::SecretKey>> {
    read_secret_text(secrets.node_key.as_ref().map(|x| &x.0))
}

fn read_secret_text<T: TextFmt>(text: Option<&Secret<String>>) -> anyhow::Result<Option<T>> {
    text.map(|text| Text::new(text.expose_secret()).decode())
        .transpose()
        .map_err(|_| anyhow::format_err!("invalid format"))
}
