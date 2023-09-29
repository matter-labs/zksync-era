use std::collections::{HashMap, HashSet};
use std::env;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Eq, Hash, PartialEq)]
pub struct CircuitIdRoundTuple {
    pub circuit_id: u8,
    pub aggregation_round: u8,
}

impl CircuitIdRoundTuple {
    pub fn new(circuit_id: u8, aggregation_round: u8) -> Self {
        Self {
            circuit_id,
            aggregation_round,
        }
    }
}

fn load_from_env_variable() -> HashMap<String, HashSet<CircuitIdRoundTuple>> {
    // Prepare a hash map to store the mapping of group to a vector of tuples
    let mut groups: HashMap<String, HashSet<CircuitIdRoundTuple>> = (0..=12)
        .map(|i| (format!("group_{}", i), HashSet::new()))
        .collect();

    // Separate environment variables into Circuit Id and Aggregation Round
    let mut circuit_ids = HashMap::new();
    let mut aggregation_rounds = HashMap::new();
    for (key, value) in env::vars() {
        if key.contains("_CIRCUIT_ID") {
            circuit_ids.insert(key, value);
        } else if key.contains("_AGGREGATION_ROUND") {
            aggregation_rounds.insert(key, value);
        }
    }

    // Iterate over all circuit id variables
    for (key, value_str) in circuit_ids {
        let key_parts: Vec<&str> = key.split('_').collect();
        if let (Some(group_key), Some(value), Some(index_str)) = (
            key_parts.get(4),
            value_str.parse::<u8>().ok(),
            key_parts.get(5),
        ) {
            let round_key = format!(
                "FRI_PROVER_GROUP_GROUP_{}_{}_AGGREGATION_ROUND",
                group_key, index_str
            );
            if let Some(round_str) = aggregation_rounds.get(&round_key) {
                if let Ok(round) = round_str.parse::<u8>() {
                    let tuple = CircuitIdRoundTuple::new(value, round);
                    if let Some(group) = groups.get_mut(&format!("group_{}", group_key)) {
                        group.insert(tuple);
                    }
                }
            }
        }
    }
    groups
}

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
    pub fn from_env() -> Self {
        let mut groups = load_from_env_variable();
        let config = FriProverGroupConfig {
            group_0: groups.remove("group_0").unwrap_or_default(),
            group_1: groups.remove("group_1").unwrap_or_default(),
            group_2: groups.remove("group_2").unwrap_or_default(),
            group_3: groups.remove("group_3").unwrap_or_default(),
            group_4: groups.remove("group_4").unwrap_or_default(),
            group_5: groups.remove("group_5").unwrap_or_default(),
            group_6: groups.remove("group_6").unwrap_or_default(),
            group_7: groups.remove("group_7").unwrap_or_default(),
            group_8: groups.remove("group_8").unwrap_or_default(),
            group_9: groups.remove("group_9").unwrap_or_default(),
            group_10: groups.remove("group_10").unwrap_or_default(),
            group_11: groups.remove("group_11").unwrap_or_default(),
            group_12: groups.remove("group_12").unwrap_or_default(),
        };
        config.validate();
        config
    }

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
    fn validate(&self) {
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
                    let expected_range = vec![2];
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
                    let expected_range = vec![1];
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

#[cfg(test)]
mod tests {
    use super::*;

    fn expected_config() -> FriProverGroupConfig {
        FriProverGroupConfig {
            group_0: vec![
                CircuitIdRoundTuple::new(1, 3),
                CircuitIdRoundTuple::new(2, 2),
            ]
            .into_iter()
            .collect::<HashSet<_>>(),
            group_1: vec![CircuitIdRoundTuple::new(1, 0)]
                .into_iter()
                .collect::<HashSet<_>>(),
            group_2: vec![
                CircuitIdRoundTuple::new(2, 0),
                CircuitIdRoundTuple::new(4, 0),
                CircuitIdRoundTuple::new(6, 0),
                CircuitIdRoundTuple::new(9, 0),
            ]
            .into_iter()
            .collect::<HashSet<_>>(),
            group_3: vec![CircuitIdRoundTuple::new(3, 0)]
                .into_iter()
                .collect::<HashSet<_>>(),
            group_4: vec![
                CircuitIdRoundTuple::new(11, 0),
                CircuitIdRoundTuple::new(12, 0),
                CircuitIdRoundTuple::new(13, 0),
            ]
            .into_iter()
            .collect::<HashSet<_>>(),
            group_5: vec![CircuitIdRoundTuple::new(5, 0)]
                .into_iter()
                .collect::<HashSet<_>>(),
            group_6: vec![CircuitIdRoundTuple::new(3, 1)]
                .into_iter()
                .collect::<HashSet<_>>(),
            group_7: vec![CircuitIdRoundTuple::new(7, 0)]
                .into_iter()
                .collect::<HashSet<_>>(),
            group_8: vec![CircuitIdRoundTuple::new(8, 0)]
                .into_iter()
                .collect::<HashSet<_>>(),
            group_9: vec![
                CircuitIdRoundTuple::new(12, 1),
                CircuitIdRoundTuple::new(13, 1),
                CircuitIdRoundTuple::new(14, 1),
                CircuitIdRoundTuple::new(15, 1),
            ]
            .into_iter()
            .collect::<HashSet<_>>(),
            group_10: vec![CircuitIdRoundTuple::new(10, 0)]
                .into_iter()
                .collect::<HashSet<_>>(),
            group_11: vec![
                CircuitIdRoundTuple::new(7, 1),
                CircuitIdRoundTuple::new(8, 1),
                CircuitIdRoundTuple::new(10, 1),
                CircuitIdRoundTuple::new(11, 1),
            ]
            .into_iter()
            .collect::<HashSet<_>>(),
            group_12: vec![
                CircuitIdRoundTuple::new(4, 1),
                CircuitIdRoundTuple::new(5, 1),
                CircuitIdRoundTuple::new(6, 1),
                CircuitIdRoundTuple::new(9, 1),
            ]
            .into_iter()
            .collect::<HashSet<_>>(),
        }
    }

    #[test]
    fn from_env() {
        let groups = [
            ("FRI_PROVER_GROUP_GROUP_0_0", CircuitIdRoundTuple::new(1, 3)),
            ("FRI_PROVER_GROUP_GROUP_0_1", CircuitIdRoundTuple::new(2, 2)),
            ("FRI_PROVER_GROUP_GROUP_1_0", CircuitIdRoundTuple::new(1, 0)),
            ("FRI_PROVER_GROUP_GROUP_2_0", CircuitIdRoundTuple::new(2, 0)),
            ("FRI_PROVER_GROUP_GROUP_2_1", CircuitIdRoundTuple::new(4, 0)),
            ("FRI_PROVER_GROUP_GROUP_2_2", CircuitIdRoundTuple::new(6, 0)),
            ("FRI_PROVER_GROUP_GROUP_2_3", CircuitIdRoundTuple::new(9, 0)),
            ("FRI_PROVER_GROUP_GROUP_3_0", CircuitIdRoundTuple::new(3, 0)),
            (
                "FRI_PROVER_GROUP_GROUP_4_0",
                CircuitIdRoundTuple::new(11, 0),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_4_1",
                CircuitIdRoundTuple::new(12, 0),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_4_2",
                CircuitIdRoundTuple::new(13, 0),
            ),
            ("FRI_PROVER_GROUP_GROUP_5_0", CircuitIdRoundTuple::new(5, 0)),
            ("FRI_PROVER_GROUP_GROUP_6_0", CircuitIdRoundTuple::new(3, 1)),
            ("FRI_PROVER_GROUP_GROUP_7_0", CircuitIdRoundTuple::new(7, 0)),
            ("FRI_PROVER_GROUP_GROUP_8_0", CircuitIdRoundTuple::new(8, 0)),
            (
                "FRI_PROVER_GROUP_GROUP_9_0",
                CircuitIdRoundTuple::new(12, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_9_1",
                CircuitIdRoundTuple::new(13, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_9_2",
                CircuitIdRoundTuple::new(14, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_9_3",
                CircuitIdRoundTuple::new(15, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_10_0",
                CircuitIdRoundTuple::new(10, 0),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_11_0",
                CircuitIdRoundTuple::new(7, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_11_1",
                CircuitIdRoundTuple::new(8, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_11_2",
                CircuitIdRoundTuple::new(10, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_11_3",
                CircuitIdRoundTuple::new(11, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_12_0",
                CircuitIdRoundTuple::new(4, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_12_1",
                CircuitIdRoundTuple::new(5, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_12_2",
                CircuitIdRoundTuple::new(6, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_12_3",
                CircuitIdRoundTuple::new(9, 1),
            ),
        ];

        for (key_base, circuit_round_tuple) in &groups {
            let circuit_id_key = format!("{}_CIRCUIT_ID", key_base);
            let aggregation_round_key = format!("{}_AGGREGATION_ROUND", key_base);
            env::set_var(&circuit_id_key, circuit_round_tuple.circuit_id.to_string());
            env::set_var(
                &aggregation_round_key,
                circuit_round_tuple.aggregation_round.to_string(),
            );
        }

        let actual = FriProverGroupConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
