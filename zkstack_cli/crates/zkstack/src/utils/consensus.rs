use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use zkstack_cli_config::{raw::PatchedConfig, ChainConfig};
use zksync_consensus_crypto::{Text, TextFmt};
use zksync_consensus_roles::{attester, node, validator};

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

/// Mirrors key–address pair used in the consensus config.
#[derive(Debug, Serialize)]
pub(crate) struct KeyAndAddress {
    pub key: String,
    pub addr: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Weighted {
    key: String,
    weight: u64,
}

impl Weighted {
    fn new(key: String, weight: u64) -> Self {
        Self { key, weight }
    }
}

pub(crate) fn read_attester_committee_yaml(
    raw_yaml: serde_yaml::Value,
) -> anyhow::Result<attester::Committee> {
    #[derive(Debug, Deserialize)]
    struct SetAttesterCommitteeFile {
        attesters: Vec<Weighted>,
    }

    let file: SetAttesterCommitteeFile =
        serde_yaml::from_value(raw_yaml).context("invalid attester committee format")?;
    let attesters: Vec<_> = file
        .attesters
        .iter()
        .enumerate()
        .map(|(i, v)| {
            Ok(attester::WeightedAttester {
                key: Text::new(&v.key).decode().context("key").context(i)?,
                weight: v.weight,
            })
        })
        .collect::<anyhow::Result<_>>()
        .context("attesters")?;
    attester::Committee::new(attesters).context("Committee::new()")
}

pub fn set_genesis_specs(
    general: &mut PatchedConfig,
    chain_config: &ChainConfig,
    consensus_keys: &ConsensusSecretKeys,
) -> anyhow::Result<()> {
    let public_keys = get_consensus_public_keys(consensus_keys);
    let validator_key = public_keys.validator_key.encode();
    let attester_key = public_keys.attester_key.encode();
    let leader = validator_key.clone();

    general.insert(
        "consensus.genesis_spec.chain_id",
        chain_config.chain_id.as_u64(),
    )?;
    general.insert("consensus.genesis_spec.protocol_version", 1_u64)?;
    general.insert_yaml(
        "consensus.genesis_spec.validators",
        [Weighted::new(validator_key, 1)],
    )?;
    general.insert_yaml(
        "consensus.genesis_spec.attesters",
        [Weighted::new(attester_key, 1)],
    )?;
    general.insert("consensus.genesis_spec.leader", leader)?;
    Ok(())
}

pub(crate) fn set_consensus_secrets(
    secrets: &mut PatchedConfig,
    consensus_keys: &ConsensusSecretKeys,
) -> anyhow::Result<()> {
    let validator_key = consensus_keys.validator_key.encode();
    let attester_key = consensus_keys.attester_key.encode();
    let node_key = consensus_keys.node_key.encode();
    secrets.insert("consensus.validator_key", validator_key)?;
    secrets.insert("consensus.attester_key", attester_key)?;
    secrets.insert("consensus.node_key", node_key)?;
    Ok(())
}

pub fn node_public_key(secret_key: &str) -> anyhow::Result<String> {
    let secret_key: node::SecretKey = Text::new(secret_key)
        .decode()
        .context("invalid node key format")?;
    Ok(secret_key.public().encode())
}
