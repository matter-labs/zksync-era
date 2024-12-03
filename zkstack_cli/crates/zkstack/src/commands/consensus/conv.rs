use anyhow::Context as _;
use zksync_config::configs::consensus as config;
use zksync_consensus_crypto::TextFmt as _;
use zksync_consensus_roles::attester;
use zksync_protobuf::{ProtoFmt, ProtoRepr};

use super::proto;
use crate::utils::consensus::parse_attester_committee;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SetAttesterCommitteeFile {
    pub attesters: attester::Committee,
}

impl ProtoFmt for SetAttesterCommitteeFile {
    type Proto = proto::SetAttesterCommitteeFile;

    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        // zksync_config was not allowed to depend on consensus crates,
        // therefore to parse the config we need to go through the intermediate
        // representation of consensus types defined in zksync_config.
        let attesters: Vec<_> = r
            .attesters
            .iter()
            .map(|x| x.read())
            .collect::<Result<_, _>>()
            .context("attesters")?;
        Ok(Self {
            attesters: parse_attester_committee(&attesters)?,
        })
    }

    fn build(&self) -> Self::Proto {
        Self::Proto {
            attesters: self
                .attesters
                .iter()
                .map(|a| {
                    ProtoRepr::build(&config::WeightedAttester {
                        key: config::AttesterPublicKey(a.key.encode()),
                        weight: a.weight,
                    })
                })
                .collect(),
        }
    }
}
