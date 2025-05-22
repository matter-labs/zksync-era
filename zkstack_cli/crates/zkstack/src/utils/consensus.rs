use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use zksync_consensus_crypto::{Text, TextFmt};
use zksync_consensus_roles::{node, validator};

pub(crate) fn read_validator_committee_yaml(
    raw_yaml: serde_yaml::Value,
) -> anyhow::Result<(Vec<validator::ValidatorInfo>, validator::LeaderSelection)> {
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

    let committee = validator::Schedule::new(committee_validators.into_iter())?;

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

pub fn node_public_key(secret_key: &str) -> anyhow::Result<String> {
    let secret_key: node::SecretKey = Text::new(secret_key)
        .decode()
        .context("invalid node key format")?;
    Ok(secret_key.public().encode())
}
