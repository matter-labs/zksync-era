use std::collections::HashSet;

use serde::Deserialize;
use zksync_basic_types::basic_fri_types::{AggregationRound, CircuitIdRoundTuple};

/// Configuration for the grouping of specialized provers.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriProverGroupConfig {
    pub group_0: HashSet<CircuitIdRoundTuple>,
    pub group_1: HashSet<CircuitIdRoundTuple>,
    pub group_2: HashSet<CircuitIdRoundTuple>,
    pub group_3: HashSet<CircuitIdRoundTuple>,
    pub group_4: HashSet<CircuitIdRoundTuple>,
    pub group_5: HashSet<CircuitIdRoundTuple>,
    pub group_6: HashSet<CircuitIdRoundTuple>,
    pub group_7: HashSet<CircuitIdRoundTuple>,
    pub group_8: HashSet<CircuitIdRoundTuple>,
    pub group_9: HashSet<CircuitIdRoundTuple>,
    pub group_10: HashSet<CircuitIdRoundTuple>,
    pub group_11: HashSet<CircuitIdRoundTuple>,
    pub group_12: HashSet<CircuitIdRoundTuple>,
    pub group_13: HashSet<CircuitIdRoundTuple>,
    pub group_14: HashSet<CircuitIdRoundTuple>,
}
impl FriProverGroupConfig {
    pub fn get_circuit_ids_for_group_id(&self, group_id: u8) -> Option<Vec<CircuitIdRoundTuple>> {
        match group_id {
            0 => Some(self.group_0.clone().into_iter().collect()),
            1 => Some(self.group_1.clone().into_iter().collect()),
            2 => Some(self.group_2.clone().into_iter().collect()),
            3 => Some(self.group_3.clone().into_iter().collect()),
            4 => Some(self.group_4.clone().into_iter().collect()),
            5 => Some(self.group_5.clone().into_iter().collect()),
            6 => Some(self.group_6.clone().into_iter().collect()),
            7 => Some(self.group_7.clone().into_iter().collect()),
            8 => Some(self.group_8.clone().into_iter().collect()),
            9 => Some(self.group_9.clone().into_iter().collect()),
            10 => Some(self.group_10.clone().into_iter().collect()),
            11 => Some(self.group_11.clone().into_iter().collect()),
            12 => Some(self.group_12.clone().into_iter().collect()),
            13 => Some(self.group_13.clone().into_iter().collect()),
            14 => Some(self.group_14.clone().into_iter().collect()),
            _ => None,
        }
    }

    pub fn get_group_id_for_circuit_id_and_aggregation_round(
        &self,
        circuit_id: u8,
        aggregation_round: u8,
    ) -> Option<u8> {
        let configs = [
            &self.group_0,
            &self.group_1,
            &self.group_2,
            &self.group_3,
            &self.group_4,
            &self.group_5,
            &self.group_6,
            &self.group_7,
            &self.group_8,
            &self.group_9,
            &self.group_10,
            &self.group_11,
            &self.group_12,
            &self.group_13,
            &self.group_14,
        ];
        configs
            .iter()
            .enumerate()
            .find(|(_, group)| {
                group.contains(&CircuitIdRoundTuple::new(circuit_id, aggregation_round))
            })
            .map(|(group_id, _)| group_id as u8)
    }

    pub fn get_all_circuit_ids(&self) -> Vec<CircuitIdRoundTuple> {
        (0..15)
            .filter_map(|group_id| self.get_circuit_ids_for_group_id(group_id))
            .flatten()
            .collect()
    }

    /// check all_circuit ids present exactly once
    /// and For each aggregation round, check that the circuit ids are in the correct range.
    /// For example, in aggregation round 0, the circuit ids should be 1 to 15 + 255 (EIP4844).
    /// In aggregation round 1, the circuit ids should be 3 to 18.
    /// In aggregation round 2, the circuit ids should be 2.
    /// In aggregation round 3, the circuit ids should be 255.
    /// In aggregation round 4, the circuit ids should be 1.
    pub fn validate(&self) -> anyhow::Result<()> {
        let groups = [
            &self.group_0,
            &self.group_1,
            &self.group_2,
            &self.group_3,
            &self.group_4,
            &self.group_5,
            &self.group_6,
            &self.group_7,
            &self.group_8,
            &self.group_9,
            &self.group_10,
            &self.group_11,
            &self.group_12,
            &self.group_13,
            &self.group_14,
        ];
        let mut expected_circuit_ids: HashSet<_> = AggregationRound::ALL_ROUNDS
            .into_iter()
            .flat_map(|r| r.circuit_ids())
            .collect();

        let mut provided_circuit_ids = HashSet::new();
        for (group_id, group) in groups.iter().enumerate() {
            for circuit_id_round in group.iter() {
                // Make sure that it's a known circuit.
                if !expected_circuit_ids.contains(circuit_id_round) {
                    anyhow::bail!(
                        "Group {} contains unexpected circuit id: {:?}",
                        group_id,
                        circuit_id_round
                    );
                }
                // Remove this circuit from the expected set: later we will check that all circuits
                // are present.
                expected_circuit_ids.remove(circuit_id_round);

                // Make sure that the circuit is not duplicated.
                if provided_circuit_ids.contains(circuit_id_round) {
                    anyhow::bail!(
                        "Group {} contains duplicate circuit id: {:?}",
                        group_id,
                        circuit_id_round
                    );
                }
                provided_circuit_ids.insert(circuit_id_round.clone());
            }
        }
        // All the circuit IDs should have been removed from the expected set.
        if !expected_circuit_ids.is_empty() {
            anyhow::bail!(
                "Some circuit ids are missing from the groups: {:?}",
                expected_circuit_ids
            );
        }

        Ok(())
    }
}
