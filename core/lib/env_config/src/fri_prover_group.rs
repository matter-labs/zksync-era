use std::{
    collections::{HashMap, HashSet},
    env,
};

use zksync_basic_types::basic_fri_types::CircuitIdRoundTuple;
use zksync_config::configs::fri_prover_group::FriProverGroupConfig;

use crate::FromEnv;

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
                CircuitIdRoundTuple::new(255, 0),
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
            (
                "FRI_PROVER_GROUP_GROUP_4_3",
                CircuitIdRoundTuple::new(255, 0),
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

        let actual = FriProverGroupConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }

    #[test]
    fn get_group_id_for_circuit_id_and_aggregation_round() {
        let fri_prover_group_config = expected_config();

        assert_eq!(
            Some(0),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(1, 3)
        );
        assert_eq!(
            Some(0),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(2, 2)
        );

        assert_eq!(
            Some(1),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(1, 0)
        );

        assert_eq!(
            Some(2),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(2, 0)
        );
        assert_eq!(
            Some(2),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(4, 0)
        );

        assert_eq!(
            Some(3),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(3, 0)
        );

        assert_eq!(
            Some(4),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(11, 0)
        );
        assert_eq!(
            Some(4),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(12, 0)
        );

        assert_eq!(
            Some(4),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(13, 0)
        );

        assert_eq!(
            Some(4),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(255, 0)
        );

        assert_eq!(
            Some(5),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(5, 0)
        );

        assert_eq!(
            Some(6),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(3, 1)
        );

        assert_eq!(
            Some(7),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(7, 0)
        );

        assert_eq!(
            Some(8),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(8, 0)
        );

        assert_eq!(
            Some(9),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(12, 1)
        );

        assert_eq!(
            Some(10),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(10, 0)
        );

        assert_eq!(
            Some(11),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(7, 1)
        );

        assert_eq!(
            Some(12),
            fri_prover_group_config.get_group_id_for_circuit_id_and_aggregation_round(4, 1)
        );

        assert!(fri_prover_group_config
            .get_group_id_for_circuit_id_and_aggregation_round(19, 0)
            .is_none());
    }
}
