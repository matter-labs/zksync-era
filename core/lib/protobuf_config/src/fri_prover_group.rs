use std::collections::HashSet;

use anyhow::Context as _;
use zksync_basic_types::basic_fri_types::CircuitIdRoundTuple;
use zksync_config::configs;
use zksync_protobuf::required;

use crate::{proto, repr::ProtoRepr};

impl ProtoRepr for proto::CircuitIdRoundTuple {
    type Type = CircuitIdRoundTuple;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            circuit_id: required(&self.circuit_id)
                .and_then(|x| Ok((*x).try_into()?))
                .context("circuit_id")?,
            aggregation_round: required(&self.aggregation_round)
                .and_then(|x| Ok((*x).try_into()?))
                .context("aggregation_round")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            circuit_id: Some(this.circuit_id.into()),
            aggregation_round: Some(this.aggregation_round.into()),
        }
    }
}

fn read_vec(v: &[proto::CircuitIdRoundTuple]) -> anyhow::Result<HashSet<CircuitIdRoundTuple>> {
    v.iter()
        .enumerate()
        .map(|(i, x)| x.read().context(i))
        .collect()
}

fn build_vec(v: &HashSet<CircuitIdRoundTuple>) -> Vec<proto::CircuitIdRoundTuple> {
    let mut v: Vec<_> = v.iter().cloned().collect();
    v.sort();
    v.iter().map(ProtoRepr::build).collect()
}

impl ProtoRepr for proto::FriProverGroup {
    type Type = configs::fri_prover_group::FriProverGroupConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            group_0: read_vec(&self.group_0).context("group_0")?,
            group_1: read_vec(&self.group_1).context("group_1")?,
            group_2: read_vec(&self.group_2).context("group_2")?,
            group_3: read_vec(&self.group_3).context("group_3")?,
            group_4: read_vec(&self.group_4).context("group_4")?,
            group_5: read_vec(&self.group_5).context("group_5")?,
            group_6: read_vec(&self.group_6).context("group_6")?,
            group_7: read_vec(&self.group_7).context("group_7")?,
            group_8: read_vec(&self.group_8).context("group_8")?,
            group_9: read_vec(&self.group_9).context("group_9")?,
            group_10: read_vec(&self.group_10).context("group_10")?,
            group_11: read_vec(&self.group_11).context("group_11")?,
            group_12: read_vec(&self.group_12).context("group_12")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            group_0: build_vec(&this.group_0),
            group_1: build_vec(&this.group_1),
            group_2: build_vec(&this.group_2),
            group_3: build_vec(&this.group_3),
            group_4: build_vec(&this.group_4),
            group_5: build_vec(&this.group_5),
            group_6: build_vec(&this.group_6),
            group_7: build_vec(&this.group_7),
            group_8: build_vec(&this.group_8),
            group_9: build_vec(&this.group_9),
            group_10: build_vec(&this.group_10),
            group_11: build_vec(&this.group_11),
            group_12: build_vec(&this.group_12),
        }
    }
}
