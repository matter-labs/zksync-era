use anyhow::Context as _;
use serde::Deserialize;
use zkstack_cli_config::{
    ChainConfig, ConsensusGenesisSpecs, GeneralConfigPatch, RawConsensusKeys, SecretsConfigPatch,
    Weighted,
};
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
    general: &mut GeneralConfigPatch,
    chain_config: &ChainConfig,
    consensus_keys: &ConsensusSecretKeys,
) -> anyhow::Result<()> {
    let public_keys = get_consensus_public_keys(consensus_keys);
    let validator_key = public_keys.validator_key.encode();
    let attester_key = public_keys.attester_key.encode();
    general.set_consensus_specs(ConsensusGenesisSpecs {
        chain_id: chain_config.chain_id,
        validators: vec![Weighted::new(validator_key.clone(), 1)],
        attesters: vec![Weighted::new(attester_key, 1)],
        leader: validator_key,
    })
}

pub(crate) fn set_consensus_secrets(
    secrets: &mut SecretsConfigPatch,
    consensus_keys: &ConsensusSecretKeys,
) -> anyhow::Result<()> {
    secrets.set_consensus_keys(RawConsensusKeys {
        validator: consensus_keys.validator_key.encode(),
        attester: consensus_keys.attester_key.encode(),
        node: consensus_keys.node_key.encode(),
    })
}

pub fn node_public_key(secret_key: &str) -> anyhow::Result<String> {
    let secret_key: node::SecretKey = Text::new(secret_key)
        .decode()
        .context("invalid node key format")?;
    Ok(secret_key.public().encode())
}
