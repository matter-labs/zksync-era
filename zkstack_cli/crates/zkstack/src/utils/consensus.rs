use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use zksync_consensus_crypto::{Text, TextFmt};
use zksync_consensus_roles::{node, validator};

pub(crate) fn read_validator_schedule_yaml(
    raw_yaml: serde_yaml::Value,
) -> anyhow::Result<(
    validator::Schedule,
    Vec<ValidatorWithPop>,
    LeaderSelectionInFile,
)> {
    let file: SetValidatorScheduleFile =
        serde_yaml::from_value(raw_yaml).context("invalid validator schedule file format")?;

    let mut validators = Vec::with_capacity(file.validators.len());
    let mut validators_with_pop = Vec::with_capacity(file.validators.len());

    for v in &file.validators {
        let key: validator::PublicKey = Text::new(&v.key).decode().context("key")?;

        validators.push(validator::ValidatorInfo {
            key: key.clone(),
            weight: v.weight,
            leader: v.leader,
        });

        validators_with_pop.push(ValidatorWithPop {
            key,
            pop: Text::new(&v.pop).decode().context("pop")?,
            weight: v.weight,
            leader: v.leader,
        });
    }

    let leader_selection = validator::LeaderSelection {
        frequency: file.leader_selection.frequency,
        mode: if file.leader_selection.weighted {
            validator::LeaderSelectionMode::Weighted
        } else {
            validator::LeaderSelectionMode::RoundRobin
        },
    };

    let schedule = validator::Schedule::new(validators, leader_selection)?;

    Ok((schedule, validators_with_pop, file.leader_selection))
}

/// This is the file that contains the validator schedule.
/// It is used to set the validator schedule in the consensus registry.
#[derive(Debug, Deserialize)]
pub(crate) struct SetValidatorScheduleFile {
    validators: Vec<ValidatorInFile>,
    leader_selection: LeaderSelectionInFile,
}

/// This represents a validator with its proof of possession in a YAML file.
/// The proof of possession is necessary because we don't trust the validator to provide
/// a valid key.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ValidatorInFile {
    pub(crate) key: String,
    pub(crate) pop: String,
    pub(crate) weight: u64,
    pub(crate) leader: bool,
}

/// This represents the leader selection parameters in the YAML file.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct LeaderSelectionInFile {
    pub(crate) frequency: u64,
    pub(crate) weighted: bool,
}

/// This represents a validator in the consensus registry.
/// The proof of possession is necessary because we don't trust the validator to provide
/// a valid key.
#[derive(Debug)]
pub(crate) struct ValidatorWithPop {
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
