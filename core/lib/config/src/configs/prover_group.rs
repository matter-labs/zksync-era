use serde::Deserialize;

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
