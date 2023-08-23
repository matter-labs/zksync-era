use serde::Deserialize;

use super::envy_load;

/// Configuration for the grouping of specialized provers.
/// This config would be used by circuit-synthesizer and provers.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProverGroupConfig {
    pub group_0_circuit_ids: Vec<u8>,
    pub group_1_circuit_ids: Vec<u8>,
    pub group_2_circuit_ids: Vec<u8>,
    pub group_3_circuit_ids: Vec<u8>,
    pub group_4_circuit_ids: Vec<u8>,
    pub group_5_circuit_ids: Vec<u8>,
    pub group_6_circuit_ids: Vec<u8>,
    pub group_7_circuit_ids: Vec<u8>,
    pub group_8_circuit_ids: Vec<u8>,
    pub group_9_circuit_ids: Vec<u8>,
    pub region_read_url: String,
    // This is used while running the provers/synthesizer in non-gcp cloud env.
    pub region_override: Option<String>,
    pub zone_read_url: String,
    // This is used while running the provers/synthesizer in non-gcp cloud env.
    pub zone_override: Option<String>,
    pub synthesizer_per_gpu: u16,
}

impl ProverGroupConfig {
    pub fn from_env() -> Self {
        envy_load("prover_group", "PROVER_GROUP_")
    }

    pub fn get_circuit_ids_for_group_id(&self, group_id: u8) -> Option<Vec<u8>> {
        match group_id {
            0 => Some(self.group_0_circuit_ids.clone()),
            1 => Some(self.group_1_circuit_ids.clone()),
            2 => Some(self.group_2_circuit_ids.clone()),
            3 => Some(self.group_3_circuit_ids.clone()),
            4 => Some(self.group_4_circuit_ids.clone()),
            5 => Some(self.group_5_circuit_ids.clone()),
            6 => Some(self.group_6_circuit_ids.clone()),
            7 => Some(self.group_7_circuit_ids.clone()),
            8 => Some(self.group_8_circuit_ids.clone()),
            9 => Some(self.group_9_circuit_ids.clone()),
            _ => None,
        }
    }

    pub fn is_specialized_group_id(&self, group_id: u8) -> bool {
        group_id <= 9
    }

    pub fn get_group_id_for_circuit_id(&self, circuit_id: u8) -> Option<u8> {
        let configs = [
            &self.group_0_circuit_ids,
            &self.group_1_circuit_ids,
            &self.group_2_circuit_ids,
            &self.group_3_circuit_ids,
            &self.group_4_circuit_ids,
            &self.group_5_circuit_ids,
            &self.group_6_circuit_ids,
            &self.group_7_circuit_ids,
            &self.group_8_circuit_ids,
            &self.group_9_circuit_ids,
        ];
        configs
            .iter()
            .enumerate()
            .find(|(_, group)| group.contains(&circuit_id))
            .map(|(group_id, _)| group_id as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ProverGroupConfig {
        ProverGroupConfig {
            group_0_circuit_ids: vec![0, 18],
            group_1_circuit_ids: vec![1, 4],
            group_2_circuit_ids: vec![2, 5],
            group_3_circuit_ids: vec![6, 7],
            group_4_circuit_ids: vec![8, 9],
            group_5_circuit_ids: vec![10, 11],
            group_6_circuit_ids: vec![12, 13],
            group_7_circuit_ids: vec![14, 15],
            group_8_circuit_ids: vec![16, 17],
            group_9_circuit_ids: vec![3],
            region_read_url: "http://metadata.google.internal/computeMetadata/v1/instance/attributes/cluster-location".to_string(),
            region_override: Some("us-central-1".to_string()),
            zone_read_url: "http://metadata.google.internal/computeMetadata/v1/instance/zone".to_string(),
            zone_override: Some("us-central-1-b".to_string()),
            synthesizer_per_gpu: 10,
        }
    }

    const CONFIG: &str = r#"
        PROVER_GROUP_GROUP_0_CIRCUIT_IDS="0,18"
        PROVER_GROUP_GROUP_1_CIRCUIT_IDS="1,4"
        PROVER_GROUP_GROUP_2_CIRCUIT_IDS="2,5"
        PROVER_GROUP_GROUP_3_CIRCUIT_IDS="6,7"
        PROVER_GROUP_GROUP_4_CIRCUIT_IDS="8,9"
        PROVER_GROUP_GROUP_5_CIRCUIT_IDS="10,11"
        PROVER_GROUP_GROUP_6_CIRCUIT_IDS="12,13"
        PROVER_GROUP_GROUP_7_CIRCUIT_IDS="14,15"
        PROVER_GROUP_GROUP_8_CIRCUIT_IDS="16,17"
        PROVER_GROUP_GROUP_9_CIRCUIT_IDS="3"
        PROVER_GROUP_REGION_READ_URL="http://metadata.google.internal/computeMetadata/v1/instance/attributes/cluster-location"
        PROVER_GROUP_REGION_OVERRIDE="us-central-1"
        PROVER_GROUP_ZONE_READ_URL="http://metadata.google.internal/computeMetadata/v1/instance/zone"
        PROVER_GROUP_ZONE_OVERRIDE="us-central-1-b"
        PROVER_GROUP_SYNTHESIZER_PER_GPU="10"
    "#;

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        lock.set_env(CONFIG);
        let actual = ProverGroupConfig::from_env();
        assert_eq!(actual, expected_config());
    }

    #[test]
    fn get_group_id_for_circuit_id() {
        let prover_group_config = expected_config();

        assert_eq!(Some(0), prover_group_config.get_group_id_for_circuit_id(0));
        assert_eq!(Some(0), prover_group_config.get_group_id_for_circuit_id(18));

        assert_eq!(Some(1), prover_group_config.get_group_id_for_circuit_id(1));
        assert_eq!(Some(1), prover_group_config.get_group_id_for_circuit_id(4));

        assert_eq!(Some(2), prover_group_config.get_group_id_for_circuit_id(2));
        assert_eq!(Some(2), prover_group_config.get_group_id_for_circuit_id(5));

        assert_eq!(Some(3), prover_group_config.get_group_id_for_circuit_id(6));
        assert_eq!(Some(3), prover_group_config.get_group_id_for_circuit_id(7));

        assert_eq!(Some(4), prover_group_config.get_group_id_for_circuit_id(8));
        assert_eq!(Some(4), prover_group_config.get_group_id_for_circuit_id(9));

        assert_eq!(Some(5), prover_group_config.get_group_id_for_circuit_id(10));
        assert_eq!(Some(5), prover_group_config.get_group_id_for_circuit_id(11));

        assert_eq!(Some(6), prover_group_config.get_group_id_for_circuit_id(12));
        assert_eq!(Some(6), prover_group_config.get_group_id_for_circuit_id(13));

        assert_eq!(Some(7), prover_group_config.get_group_id_for_circuit_id(14));
        assert_eq!(Some(7), prover_group_config.get_group_id_for_circuit_id(15));

        assert_eq!(Some(8), prover_group_config.get_group_id_for_circuit_id(16));
        assert_eq!(Some(8), prover_group_config.get_group_id_for_circuit_id(17));

        assert_eq!(Some(9), prover_group_config.get_group_id_for_circuit_id(3));
        assert!(prover_group_config
            .get_group_id_for_circuit_id(19)
            .is_none());
    }

    #[test]
    fn get_circuit_ids_for_group_id() {
        let prover_group_config = expected_config();

        assert_eq!(
            Some(vec![0, 18]),
            prover_group_config.get_circuit_ids_for_group_id(0)
        );
        assert_eq!(
            Some(vec![1, 4]),
            prover_group_config.get_circuit_ids_for_group_id(1)
        );
        assert_eq!(
            Some(vec![2, 5]),
            prover_group_config.get_circuit_ids_for_group_id(2)
        );
        assert_eq!(
            Some(vec![6, 7]),
            prover_group_config.get_circuit_ids_for_group_id(3)
        );
        assert_eq!(
            Some(vec![8, 9]),
            prover_group_config.get_circuit_ids_for_group_id(4)
        );
        assert_eq!(
            Some(vec![10, 11]),
            prover_group_config.get_circuit_ids_for_group_id(5)
        );
        assert_eq!(
            Some(vec![12, 13]),
            prover_group_config.get_circuit_ids_for_group_id(6)
        );
        assert_eq!(
            Some(vec![14, 15]),
            prover_group_config.get_circuit_ids_for_group_id(7)
        );
        assert_eq!(
            Some(vec![16, 17]),
            prover_group_config.get_circuit_ids_for_group_id(8)
        );
        assert_eq!(
            Some(vec![3]),
            prover_group_config.get_circuit_ids_for_group_id(9)
        );
        assert!(prover_group_config
            .get_circuit_ids_for_group_id(10)
            .is_none());
    }
}
