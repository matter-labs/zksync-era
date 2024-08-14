use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use zksync_basic_types::basic_fri_types::CircuitIdRoundTuple;

/// Configuration for the grouping of specialized provers.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
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
        let mut rounds: Vec<Vec<CircuitIdRoundTuple>> = vec![Vec::new(); 5];
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
        for group in groups {
            for circuit_round in group {
                let round = match rounds.get_mut(circuit_round.aggregation_round as usize) {
                    Some(round) => round,
                    None => anyhow::bail!(
                        "Invalid aggregation round {}.",
                        circuit_round.aggregation_round
                    ),
                };
                round.push(circuit_round.clone());
            }
        }

        for (round, round_data) in rounds.iter().enumerate() {
            let circuit_ids: Vec<u8> = round_data.iter().map(|x| x.circuit_id).collect();
            let unique_circuit_ids: HashSet<u8> = circuit_ids.iter().copied().collect();
            let duplicates: HashSet<u8> = circuit_ids
                .iter()
                .filter(|id| circuit_ids.iter().filter(|x| x == id).count() > 1)
                .copied()
                .collect();

            let (missing_ids, not_in_range, expected_circuits_description) = match round {
                0 => {
                    let mut expected_range: Vec<_> = (1..=15).collect();
                    expected_range.push(255);
                    let missing_ids: Vec<_> = expected_range
                        .iter()
                        .copied()
                        .filter(|id| !circuit_ids.contains(id))
                        .collect();

                    let not_in_range: Vec<_> = circuit_ids
                        .iter()
                        .filter(|&id| !expected_range.contains(id))
                        .collect();
                    (missing_ids, not_in_range, "circuit IDs 1 to 15 and 255")
                }
                1 => {
                    let expected_range: Vec<_> = (3..=18).collect();
                    let missing_ids: Vec<_> = expected_range
                        .iter()
                        .copied()
                        .filter(|id| !circuit_ids.contains(id))
                        .collect();
                    let not_in_range: Vec<_> = circuit_ids
                        .iter()
                        .filter(|&id| !expected_range.contains(id))
                        .collect();
                    (missing_ids, not_in_range, "circuit IDs 3 to 18")
                }
                2 => {
                    let expected_range: Vec<_> = vec![2];
                    let missing_ids: Vec<_> = expected_range
                        .iter()
                        .copied()
                        .filter(|id| !circuit_ids.contains(id))
                        .collect();
                    let not_in_range: Vec<_> = circuit_ids
                        .iter()
                        .filter(|&id| !expected_range.contains(id))
                        .collect();
                    (missing_ids, not_in_range, "circuit ID 2")
                }
                3 => {
                    let expected_range: Vec<_> = vec![255];
                    let missing_ids: Vec<_> = expected_range
                        .iter()
                        .copied()
                        .filter(|id| !circuit_ids.contains(id))
                        .collect();
                    let not_in_range: Vec<_> = circuit_ids
                        .iter()
                        .filter(|&id| !expected_range.contains(id))
                        .collect();
                    (missing_ids, not_in_range, "circuit ID 255")
                }
                4 => {
                    let expected_range: Vec<_> = vec![1];
                    let missing_ids: Vec<_> = expected_range
                        .iter()
                        .copied()
                        .filter(|id| !circuit_ids.contains(id))
                        .collect();
                    let not_in_range: Vec<_> = circuit_ids
                        .iter()
                        .filter(|&id| !expected_range.contains(id))
                        .collect();
                    (missing_ids, not_in_range, "circuit ID 1")
                }
                _ => {
                    anyhow::bail!("Unknown round {}", round);
                }
            };
            if !missing_ids.is_empty() {
                anyhow::bail!("Circuit IDs for round {round} are missing: {missing_ids:?}");
            }
            if circuit_ids.len() != unique_circuit_ids.len() {
                anyhow::bail!("Circuit IDs: {duplicates:?} should be unique for round {round}.",);
            }
            if !not_in_range.is_empty() {
                anyhow::bail!("Aggregation round {round} should only contain {expected_circuits_description}. Ids out of range: {not_in_range:?}");
            }
        }
        Ok(())
    }
}
