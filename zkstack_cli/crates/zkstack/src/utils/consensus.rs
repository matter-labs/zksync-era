use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use zksync_consensus_crypto::{Text, TextFmt};
use zksync_consensus_roles::{node, validator};

pub(crate) fn read_validator_committee_yaml(
    raw_yaml: serde_yaml::Value,
) -> anyhow::Result<validator::Schedule> {
    let file: SetValidatorCommitteeFile =
        serde_yaml::from_value(raw_yaml).context("invalid validator schedule file format")?;

    let mut validators = Vec::with_capacity(file.validators.len());

    for v in &file.validators {
        let key: validator::PublicKey = Text::new(&v.key).decode().context("key")?;

        validators.push(validator::ValidatorInfo {
            key: key.clone(),
            weight: v.weight,
            leader: v.leader,
        });
    }

    let leader_selection = validator::LeaderSelection {
        frequency: file.frequency,
        mode: if file.weighted {
            validator::LeaderSelectionMode::Weighted
        } else {
            validator::LeaderSelectionMode::RoundRobin
        },
    };

    let schedule = validator::Schedule::new(validators, leader_selection)?;

    Ok(schedule)
}

/// This is the file that contains the validator schedule.
/// It is used to set the validator schedule in the consensus registry.
#[derive(Debug, Deserialize)]
pub(crate) struct SetValidatorCommitteeFile {
    validators: Vec<ValidatorInFile>,
    frequency: u64,
    weighted: bool,
}

/// This represents a validator with its proof of possession in a YAML file.
/// The proof of possession is necessary because we don't trust the validator to provide
/// a valid key.
#[derive(Debug, Serialize, Deserialize)]
struct ValidatorInFile {
    key: String,
    pop: String,
    weight: u64,
    leader: bool,
}

/// This represents a validator in the consensus registry.
/// The proof of possession is necessary because we don't trust the validator to provide
/// a valid key.
#[derive(Debug)]
pub(crate) struct Validator {
    pub(crate) key: validator::PublicKey,
    pub(crate) pop: validator::ProofOfPossession,
    pub(crate) weight: u64,
    pub(crate) leader: bool,
}

pub fn node_public_key(secret_key: &str) -> anyhow::Result<String> {
    let secret_key: node::SecretKey = Text::new(secret_key)
        .decode()
        .context("invalid node key format")?;
    Ok(secret_key.public().encode())
}
