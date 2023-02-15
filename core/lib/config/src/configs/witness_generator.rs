use std::time::Duration;

// Built-in uses
// External uses
use serde::Deserialize;
// Local uses
use crate::envy_load;

/// Configuration for the witness generation
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct WitnessGeneratorConfig {
    /// Max time for witness to be generated
    pub generation_timeout_in_secs: u16,
    /// Currently only a single (largest) key is supported.
    pub initial_setup_key_path: String,
    /// https://storage.googleapis.com/universal-setup/setup_2\^26.key
    pub key_download_url: String,
    /// Max attempts for generating witness
    pub max_attempts: u32,
    /// Is sampling enabled
    pub sampling_enabled: bool,
    /// Safe prover lag to process block
    pub sampling_safe_prover_lag: Option<usize>,
    /// Max prover lag to process block
    pub sampling_max_prover_lag: Option<usize>,
    pub dump_arguments_for_blocks: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct SamplingParams {
    pub safe_prover_lag: usize,
    pub max_prover_lag: usize,
}

impl SamplingParams {
    pub fn calculate_sampling_probability(&self, prover_lag: usize) -> f64 {
        let numerator = self.max_prover_lag as f64 - prover_lag as f64;
        let denominator = (self.max_prover_lag - self.safe_prover_lag).max(1) as f64;
        (numerator / denominator).min(1f64).max(0f64)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SamplingMode {
    Enabled(SamplingParams),
    Disabled,
}

impl WitnessGeneratorConfig {
    pub fn from_env() -> Self {
        envy_load!("witness", "WITNESS_")
    }

    pub fn witness_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_in_secs as u64)
    }

    pub fn sampling_mode(&self) -> SamplingMode {
        match (
            self.sampling_enabled,
            self.sampling_safe_prover_lag,
            self.sampling_max_prover_lag,
        ) {
            (true, Some(safe_prover_lag), Some(max_prover_lag)) => {
                SamplingMode::Enabled(SamplingParams {
                    safe_prover_lag,
                    max_prover_lag,
                })
            }
            _ => SamplingMode::Disabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::set_env;

    fn expected_config() -> WitnessGeneratorConfig {
        WitnessGeneratorConfig {
            generation_timeout_in_secs: 900u16,
            initial_setup_key_path: "key".to_owned(),
            key_download_url: "value".to_owned(),
            max_attempts: 4,
            sampling_enabled: true,
            sampling_safe_prover_lag: Some(50),
            sampling_max_prover_lag: Some(300),
            dump_arguments_for_blocks: vec![2, 3],
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
        WITNESS_GENERATION_TIMEOUT_IN_SECS=900
        WITNESS_INITIAL_SETUP_KEY_PATH="key"
        WITNESS_KEY_DOWNLOAD_URL="value"
        WITNESS_MAX_ATTEMPTS=4
        WITNESS_SAMPLING_ENABLED=true
        WITNESS_SAMPLING_SAFE_PROVER_LAG=50
        WITNESS_SAMPLING_MAX_PROVER_LAG=300
        WITNESS_DUMP_ARGUMENTS_FOR_BLOCKS="2,3"
        "#;
        set_env(config);
        let actual = WitnessGeneratorConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
