use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use zkstack_cli_config::{raw::PatchedConfig, ChainConfig};
use zksync_consensus_crypto::{Text, TextFmt};
use zksync_consensus_roles::{node, validator};

pub(crate) fn read_validator_committee_yaml(
    raw_yaml: serde_yaml::Value,
) -> anyhow::Result<(validator::Committee, Vec<Validator>)> {
    let file: SetValidatorCommitteeFile =
        serde_yaml::from_value(raw_yaml).context("invalid validator committee file format")?;

    let mut committee_validators = Vec::with_capacity(file.validators.len());
    let mut validators = Vec::with_capacity(file.validators.len());

    for v in &file.validators {
        let key: validator::PublicKey = Text::new(&v.key).decode().context("key")?;

        committee_validators.push(validator::WeightedValidator {
            key: key.clone(),
            weight: v.weight,
        });

        validators.push(Validator {
            key,
            pop: Text::new(&v.pop).decode().context("pop")?,
            weight: v.weight,
        });
    }

    let committee = validator::Committee::new(committee_validators.into_iter())?;

    Ok((committee, validators))
}

/// This is the file that contains the validator committee.
/// It is used to set the validator committee in the consensus registry.
#[derive(Debug, Deserialize)]
pub(crate) struct SetValidatorCommitteeFile {
    validators: Vec<ValidatorWithPopYaml>,
}

/// This represents a validator with its proof of possession in a YAML file.
/// The proof of possession is necessary because we don't trust the validator to provide
/// a valid key.
#[derive(Debug, Serialize, Deserialize)]
struct ValidatorWithPopYaml {
    key: String,
    pop: String,
    weight: u64,
}

/// This represents a validator in the consensus registry.
/// The proof of possession is necessary because we don't trust the validator to provide
/// a valid key.
#[derive(Debug)]
pub(crate) struct Validator {
    pub(crate) key: validator::PublicKey,
    pub(crate) pop: validator::ProofOfPossession,
    pub(crate) weight: u64,
}

pub fn set_genesis_specs(
    general: &mut PatchedConfig,
    chain_config: &ChainConfig,
    consensus_keys: &ConsensusSecretKeys,
) -> anyhow::Result<()> {
    let validator_key = consensus_keys.validator_key.public().encode();
    let leader = validator_key.clone();

    general.insert(
        "consensus.genesis_spec.chain_id",
        chain_config.chain_id.as_u64(),
    )?;
    general.insert("consensus.genesis_spec.protocol_version", 1_u64)?;
    general.insert_yaml(
        "consensus.genesis_spec.validators",
        [WeightedValidatorYaml {
            key: validator_key,
            weight: 1,
        }],
    )?;
    general.insert("consensus.genesis_spec.leader", leader)?;
    Ok(())
}

pub(crate) fn set_consensus_secrets(
    secrets: &mut PatchedConfig,
    consensus_keys: &ConsensusSecretKeys,
) -> anyhow::Result<()> {
    let validator_key = consensus_keys.validator_key.encode();
    let node_key = consensus_keys.node_key.encode();
    secrets.insert("consensus.validator_key", validator_key)?;
    secrets.insert("consensus.node_key", node_key)?;
    Ok(())
}

pub fn node_public_key(secret_key: &str) -> anyhow::Result<String> {
    let secret_key: node::SecretKey = Text::new(secret_key)
        .decode()
        .context("invalid node key format")?;
    Ok(secret_key.public().encode())
}

#[derive(Debug, Clone)]
pub struct ConsensusSecretKeys {
    validator_key: validator::SecretKey,
    node_key: node::SecretKey,
}

impl ConsensusSecretKeys {
    pub fn generate() -> Self {
        Self {
            validator_key: validator::SecretKey::generate(),
            node_key: node::SecretKey::generate(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct WeightedValidatorYaml {
    key: String,
    weight: u64,
}

/// Mirrors key–address pair used in the consensus config.
#[derive(Debug, Serialize)]
pub(crate) struct KeyAndAddress {
    pub key: String,
    pub addr: String,
}
