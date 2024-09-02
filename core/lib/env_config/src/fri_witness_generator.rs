use zksync_config::configs::FriWitnessGeneratorConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for FriWitnessGeneratorConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("fri_witness", "FRI_WITNESS_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> FriWitnessGeneratorConfig {
        FriWitnessGeneratorConfig {
            generation_timeout_in_secs: 900u16,
            basic_generation_timeout_in_secs: Some(900u16),
            leaf_generation_timeout_in_secs: Some(800u16),
            node_generation_timeout_in_secs: Some(800u16),
            recursion_tip_generation_timeout_in_secs: Some(700u16),
            scheduler_generation_timeout_in_secs: Some(900u16),
            max_attempts: 4,
            last_l1_batch_to_process: None,
            shall_save_to_public_bucket: true,
            prometheus_listener_port: Some(3333u16),
            max_circuits_in_flight: 500,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            FRI_WITNESS_GENERATION_TIMEOUT_IN_SECS=900
            FRI_WITNESS_BASIC_GENERATION_TIMEOUT_IN_SECS=900
            FRI_WITNESS_LEAF_GENERATION_TIMEOUT_IN_SECS=800
            FRI_WITNESS_NODE_GENERATION_TIMEOUT_IN_SECS=800
            FRI_WITNESS_RECURSION_TIP_GENERATION_TIMEOUT_IN_SECS=700
            FRI_WITNESS_SCHEDULER_GENERATION_TIMEOUT_IN_SECS=900
            FRI_WITNESS_MAX_ATTEMPTS=4
            FRI_WITNESS_SHALL_SAVE_TO_PUBLIC_BUCKET=true
            FRI_WITNESS_PROMETHEUS_LISTENER_PORT=3333
            FRI_WITNESS_MAX_CIRCUITS_IN_FLIGHT=500
        "#;
        lock.set_env(config);

        let actual = FriWitnessGeneratorConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }

    #[test]
    fn from_env_default_timeouts() {
        let mut lock = MUTEX.lock();
        lock.remove_env(&[
            "FRI_WITNESS_LEAF_GENERATION_TIMEOUT_IN_SECS",
            "FRI_WITNESS_NODE_GENERATION_TIMEOUT_IN_SECS",
            "FRI_WITNESS_RECURSION_TIP_GENERATION_TIMEOUT_IN_SECS",
        ]);
        let config = r#"
            FRI_WITNESS_GENERATION_TIMEOUT_IN_SECS=800
            FRI_WITNESS_BASIC_GENERATION_TIMEOUT_IN_SECS=100
            FRI_WITNESS_SCHEDULER_GENERATION_TIMEOUT_IN_SECS=200
            FRI_WITNESS_MAX_ATTEMPTS=4
            FRI_WITNESS_SHALL_SAVE_TO_PUBLIC_BUCKET=true
        "#;
        lock.set_env(config);

        let actual = FriWitnessGeneratorConfig::from_env().unwrap();
        let timeouts = actual.witness_generation_timeouts();

        assert_eq!(timeouts.basic().as_secs(), 100);
        assert_eq!(timeouts.leaf().as_secs(), 800);
        assert_eq!(timeouts.node().as_secs(), 800);
        assert_eq!(timeouts.recursion_tip().as_secs(), 800);
        assert_eq!(timeouts.scheduler().as_secs(), 200);
    }
}
