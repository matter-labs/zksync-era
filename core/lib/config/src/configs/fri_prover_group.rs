use std::collections::HashSet;

use smart_config::{
    de::{Repeated, Serde},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::basic_fri_types::{AggregationRound, CircuitIdRoundTuple};

/// Configuration for the grouping of specialized provers.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct FriProverGroupConfig {
    #[config(with = Repeated(Serde![object]))]
    pub group_0: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_1: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_2: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_3: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_4: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_5: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_6: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_7: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_8: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_9: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_10: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_11: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_12: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
    pub group_13: HashSet<CircuitIdRoundTuple>,
    #[config(with = Repeated(Serde![object]))]
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

#[cfg(test)]
mod tests {
    use smart_config::{
        testing::{test, test_complete},
        Environment, Yaml,
    };

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

    #[ignore] // FIXME: requires extended parsing from env to work
    #[test]
    fn parsing_from_env() {
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

        let env = groups.iter().flat_map(|(key_base, circuit_round_tuple)| {
            let circuit_id_key = format!("{key_base}_CIRCUIT_ID");
            let aggregation_round_key = format!("{key_base}_AGGREGATION_ROUND");
            [
                (circuit_id_key, circuit_round_tuple.circuit_id.to_string()),
                (
                    aggregation_round_key,
                    circuit_round_tuple.aggregation_round.to_string(),
                ),
            ]
        });
        let env = Environment::from_iter("FRI_PROVER_GROUP_", env);

        let config: FriProverGroupConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          group_0:
          - circuit_id: 1
            aggregation_round: 4
          - circuit_id: 2
            aggregation_round: 2
          - circuit_id: 255
            aggregation_round: 0
          group_1:
          - circuit_id: 1
            aggregation_round: 0
          group_2:
          - circuit_id: 2
            aggregation_round: 0
          - circuit_id: 4
            aggregation_round: 0
          - circuit_id: 6
            aggregation_round: 0
          - circuit_id: 9
            aggregation_round: 0
          group_3:
          - circuit_id: 3
            aggregation_round: 0
          group_4:
          - circuit_id: 11
            aggregation_round: 0
          - circuit_id: 12
            aggregation_round: 0
          - circuit_id: 13
            aggregation_round: 0
          group_5:
          - circuit_id: 5
            aggregation_round: 0
          group_6:
          - circuit_id: 3
            aggregation_round: 1
          group_7:
          - circuit_id: 7
            aggregation_round: 0
          group_8:
          - circuit_id: 8
            aggregation_round: 0
          group_9:
          - circuit_id: 12
            aggregation_round: 1
          - circuit_id: 13
            aggregation_round: 1
          - circuit_id: 14
            aggregation_round: 1
          - circuit_id: 15
            aggregation_round: 1
          group_10:
          - circuit_id: 10
            aggregation_round: 0
          group_11:
          - circuit_id: 7
            aggregation_round: 1
          - circuit_id: 8
            aggregation_round: 1
          - circuit_id: 10
            aggregation_round: 1
          - circuit_id: 11
            aggregation_round: 1
          group_12:
          - circuit_id: 4
            aggregation_round: 1
          - circuit_id: 5
            aggregation_round: 1
          - circuit_id: 6
            aggregation_round: 1
          - circuit_id: 9
            aggregation_round: 1
          group_13:
          - circuit_id: 14
            aggregation_round: 0
          - circuit_id: 15
            aggregation_round: 0
          - circuit_id: 255
            aggregation_round: 3
          group_14:
          - circuit_id: 16
            aggregation_round: 1
          - circuit_id: 17
            aggregation_round: 1
          - circuit_id: 18
            aggregation_round: 1
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: FriProverGroupConfig = test(yaml).unwrap();
        config.validate().unwrap();
    }
}
