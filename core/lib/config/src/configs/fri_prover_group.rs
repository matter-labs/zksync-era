use serde::Deserialize;
use std::collections::HashSet;

use zksync_basic_types::basic_fri_types::CircuitIdRoundTuple;

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
        (0..13)
            .filter_map(|group_id| self.get_circuit_ids_for_group_id(group_id))
            .flatten()
            .collect()
    }
    /// check all_circuit ids present exactly once
    /// and For each aggregation round, check that the circuit ids are in the correct range.
    /// For example, in aggregation round 0, the circuit ids should be 1 to 13.
    /// In aggregation round 1, the circuit ids should be 3 to 15.
    /// In aggregation round 2, the circuit ids should be 2.
    /// In aggregation round 3, the circuit ids should be 1.
    pub fn validate(&self) {
        let mut rounds: Vec<Vec<CircuitIdRoundTuple>> = vec![Vec::new(); 4];
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
        ];
        for group in groups {
            for circuit_round in group {
                rounds[circuit_round.aggregation_round as usize].push(circuit_round.clone());
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

            match round {
                0 => {
                    let expected_range: Vec<_> = (1..=13).collect();
                    let missing_ids: Vec<_> = expected_range
                        .iter()
                        .filter(|id| !circuit_ids.contains(id))
                        .collect();
                    assert!(
                        missing_ids.is_empty(),
                        "Circuit IDs for round {} are missing: {:?}",
                        round,
                        missing_ids
                    );
                    assert_eq!(
                        circuit_ids.len(),
                        unique_circuit_ids.len(),
                        "Circuit IDs: {:?} should be unique for round {}.",
                        duplicates,
                        round
                    );
                    let not_in_range: Vec<_> = circuit_ids
                        .iter()
                        .filter(|&id| !expected_range.contains(id))
                        .collect();
                    assert!(not_in_range.is_empty(), "Aggregation round 0 should only contain circuit IDs 1 to 13. Ids out of range: {:?}", not_in_range);
                }
                1 => {
                    let expected_range: Vec<_> = (3..=15).collect();
                    let missing_ids: Vec<_> = expected_range
                        .iter()
                        .filter(|id| !circuit_ids.contains(id))
                        .collect();
                    assert!(
                        missing_ids.is_empty(),
                        "Circuit IDs for round {} are missing: {:?}",
                        round,
                        missing_ids
                    );
                    assert_eq!(
                        circuit_ids.len(),
                        unique_circuit_ids.len(),
                        "Circuit IDs: {:?} should be unique for round {}.",
                        duplicates,
                        round
                    );
                    let not_in_range: Vec<_> = circuit_ids
                        .iter()
                        .filter(|&id| !expected_range.contains(id))
                        .collect();
                    assert!(not_in_range.is_empty(), "Aggregation round 1 should only contain circuit IDs 3 to 15. Ids out of range: {:?}", not_in_range);
                }
                2 => {
                    let expected_range = [2];
                    let missing_ids: Vec<_> = expected_range
                        .iter()
                        .filter(|id| !circuit_ids.contains(id))
                        .collect();
                    assert!(
                        missing_ids.is_empty(),
                        "Circuit IDs for round {} are missing: {:?}",
                        round,
                        missing_ids
                    );
                    assert_eq!(
                        circuit_ids.len(),
                        unique_circuit_ids.len(),
                        "Circuit IDs: {:?} should be unique for round {}.",
                        duplicates,
                        round
                    );
                    let not_in_range: Vec<_> = circuit_ids
                        .iter()
                        .filter(|&id| !expected_range.contains(id))
                        .collect();
                    assert!(not_in_range.is_empty(), "Aggregation round 2 should only contain circuit ID 2. Ids out of range: {:?}", not_in_range);
                }
                3 => {
                    let expected_range = [1];
                    let missing_ids: Vec<_> = expected_range
                        .iter()
                        .filter(|id| !circuit_ids.contains(id))
                        .collect();
                    assert!(
                        missing_ids.is_empty(),
                        "Circuit IDs for round {} are missing: {:?}",
                        round,
                        missing_ids
                    );
                    assert_eq!(
                        circuit_ids.len(),
                        unique_circuit_ids.len(),
                        "Circuit IDs: {:?} should be unique for round {}.",
                        duplicates,
                        round
                    );
                    let not_in_range: Vec<_> = circuit_ids
                        .iter()
                        .filter(|&id| !expected_range.contains(id))
                        .collect();
                    assert!(not_in_range.is_empty(), "Aggregation round 3 should only contain circuit ID 1. Ids out of range: {:?}", not_in_range);
                }
                _ => {
                    panic!("Unknown round {}", round)
                }
            }
        }
    }
}
