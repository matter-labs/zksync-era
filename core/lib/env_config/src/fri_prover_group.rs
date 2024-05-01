use std::{
    collections::{HashMap, HashSet},
    env,
};

use zksync_basic_types::basic_fri_types::CircuitIdRoundTuple;
use zksync_config::configs::fri_prover_group::FriProverGroupConfig;

use crate::FromEnv;

fn load_from_env_variable() -> HashMap<String, HashSet<CircuitIdRoundTuple>> {
    // Prepare a hash map to store the mapping of group to a vector of tuples
    let mut groups: HashMap<String, HashSet<CircuitIdRoundTuple>> = (0..=14)
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

impl FromEnv for FriProverGroupConfig {
    fn from_env() -> anyhow::Result<Self> {
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
            group_13: groups.remove("group_13").unwrap_or_default(),
            group_14: groups.remove("group_14").unwrap_or_default(),
        };
        config.validate()?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn expected_config() -> FriProverGroupConfig {
        FriProverGroupConfig {
            group_0: vec![
                CircuitIdRoundTuple::new(1, 4),
                CircuitIdRoundTuple::new(2, 2),
                CircuitIdRoundTuple::new(255, 3),
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
            group_13: vec![
                CircuitIdRoundTuple::new(14, 0),
                CircuitIdRoundTuple::new(15, 0),
                CircuitIdRoundTuple::new(255, 0),
            ]
            .into_iter()
            .collect::<HashSet<_>>(),
            group_14: vec![
                CircuitIdRoundTuple::new(16, 1),
                CircuitIdRoundTuple::new(17, 1),
                CircuitIdRoundTuple::new(18, 1),
            ]
            .into_iter()
            .collect::<HashSet<_>>(),
        }
    }

    #[test]
    fn from_env() {
        let groups = [
            ("FRI_PROVER_GROUP_GROUP_0_0", CircuitIdRoundTuple::new(1, 4)),
            ("FRI_PROVER_GROUP_GROUP_0_1", CircuitIdRoundTuple::new(2, 2)),
            (
                "FRI_PROVER_GROUP_GROUP_0_2",
                CircuitIdRoundTuple::new(255, 3),
            ),
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
            (
                "FRI_PROVER_GROUP_GROUP_13_0",
                CircuitIdRoundTuple::new(14, 0),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_13_1",
                CircuitIdRoundTuple::new(15, 0),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_13_2",
                CircuitIdRoundTuple::new(255, 0),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_14_0",
                CircuitIdRoundTuple::new(16, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_14_1",
                CircuitIdRoundTuple::new(17, 1),
            ),
            (
                "FRI_PROVER_GROUP_GROUP_14_2",
                CircuitIdRoundTuple::new(18, 1),
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

        let actual = FriProverGroupConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }

    #[test]
    fn get_group_id_for_circuit_id_and_aggregation_round() {
        struct GroupCircuitRound(u8, u8, u8);

        let fri_prover_group_config = expected_config();

        let tests = vec![
            GroupCircuitRound(0, 1, 4),
            GroupCircuitRound(0, 2, 2),
            GroupCircuitRound(0, 255, 3),
            GroupCircuitRound(1, 1, 0),
            GroupCircuitRound(2, 2, 0),
            GroupCircuitRound(2, 4, 0),
            GroupCircuitRound(2, 6, 0),
            GroupCircuitRound(2, 9, 0),
            GroupCircuitRound(3, 3, 0),
            GroupCircuitRound(4, 11, 0),
            GroupCircuitRound(4, 12, 0),
            GroupCircuitRound(4, 13, 0),
            GroupCircuitRound(5, 5, 0),
            GroupCircuitRound(6, 3, 1),
            GroupCircuitRound(7, 7, 0),
            GroupCircuitRound(8, 8, 0),
            GroupCircuitRound(9, 12, 1),
            GroupCircuitRound(9, 13, 1),
            GroupCircuitRound(9, 14, 1),
            GroupCircuitRound(9, 15, 1),
            GroupCircuitRound(10, 10, 0),
            GroupCircuitRound(11, 7, 1),
            GroupCircuitRound(11, 8, 1),
            GroupCircuitRound(11, 10, 1),
            GroupCircuitRound(11, 11, 1),
            GroupCircuitRound(12, 4, 1),
            GroupCircuitRound(12, 5, 1),
            GroupCircuitRound(12, 6, 1),
            GroupCircuitRound(12, 9, 1),
            GroupCircuitRound(13, 14, 0),
            GroupCircuitRound(13, 15, 0),
            GroupCircuitRound(13, 255, 0),
            GroupCircuitRound(14, 16, 1),
            GroupCircuitRound(14, 17, 1),
            GroupCircuitRound(14, 18, 1),
        ];
        for test in tests {
            assert_eq!(
                Some(test.0),
                fri_prover_group_config
                    .get_group_id_for_circuit_id_and_aggregation_round(test.1, test.2)
            );
        }
        assert!(fri_prover_group_config
            .get_group_id_for_circuit_id_and_aggregation_round(19, 0)
            .is_none());
    }
}
