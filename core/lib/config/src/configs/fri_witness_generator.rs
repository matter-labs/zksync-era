use std::time::Duration;

use smart_config::{
    de::{Optional, Serde},
    metadata::TimeUnit,
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::L1BatchNumber;

/// Configuration for the fri witness generation
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct FriWitnessGeneratorConfig {
    /// Max time for witness to be generated
    #[config(default_t = 15 * TimeUnit::Minutes, with = TimeUnit::Seconds)]
    pub generation_timeout_in_secs: Duration,
    #[config(with = Optional(TimeUnit::Seconds))]
    pub basic_generation_timeout_in_secs: Option<Duration>,
    #[config(with = Optional(TimeUnit::Seconds))]
    pub leaf_generation_timeout_in_secs: Option<Duration>,
    #[config(with = Optional(TimeUnit::Seconds))]
    pub scheduler_generation_timeout_in_secs: Option<Duration>,
    #[config(with = Optional(TimeUnit::Seconds))]
    pub node_generation_timeout_in_secs: Option<Duration>,
    #[config(alias = "recursion_tip_timeout_in_secs", with = Optional(TimeUnit::Seconds))]
    pub recursion_tip_generation_timeout_in_secs: Option<Duration>,
    /// Max attempts for generating witness
    #[config(default_t = 5)]
    pub max_attempts: u32,
    // Optional l1 batch number to process block until(inclusive).
    // This parameter is used in case of performing circuit upgrades(VK/Setup keys),
    // to not let witness-generator pick new job and finish all the existing jobs with old circuit.
    #[config(with = Optional(Serde![int]))]
    pub last_l1_batch_to_process: Option<L1BatchNumber>,

    pub prometheus_listener_port: Option<u16>,

    /// This value corresponds to the maximum number of circuits kept in memory at any given time for a BWG/LWG/NWG.
    /// Acts as a throttling mechanism for circuits; the trade-off here is speed vs memory usage.
    ///
    /// BWG:
    /// With more circuits in flight, harness does not need to wait for BWG runner to process them.
    /// But every single circuit in flight eats memory (up to 50MB).
    ///
    /// LWG/NWG:
    /// Each circuit is processed in parallel.
    /// Each circuit requires downloading RECURSION_ARITY (32) proofs, each of which can be roughly estimated at 1 MB.
    /// So every single circuit should use ~32 MB of RAM + some overhead during serialization
    ///
    /// WARNING: Do NOT change this value unless you're absolutely sure you know what you're doing.
    /// It affects the performance and resource usage of WGs.
    #[config(default_t = DEFAULT_MAX_CIRCUITS_IN_FLIGHT)]
    pub max_circuits_in_flight: usize,
}

/// 500 was picked as a mid-ground between allowing enough circuits in flight to speed up BWG circuit generation,
/// whilst keeping memory as low as possible. At the moment, max size of a circuit in BWG is ~50MB.
/// This number is important when there are issues with saving circuits (network issues, service unavailability, etc.)
/// Maximum theoretic extra memory consumed by BWG is up to 25GB (50MB * 500 circuits), but in reality, worse case scenarios are closer to 5GB (the average space distribution).
/// During normal operations (> P95), this will incur an overhead of ~100MB.
const DEFAULT_MAX_CIRCUITS_IN_FLIGHT: usize = 500;

#[derive(Debug)]
pub struct WitnessGenerationTimeouts {
    basic: Duration,
    leaf: Duration,
    node: Duration,
    recursion_tip: Duration,
    scheduler: Duration,
}

impl WitnessGenerationTimeouts {
    pub fn basic(&self) -> Duration {
        self.basic
    }

    pub fn leaf(&self) -> Duration {
        self.leaf
    }

    pub fn node(&self) -> Duration {
        self.node
    }

    pub fn recursion_tip(&self) -> Duration {
        self.recursion_tip
    }

    pub fn scheduler(&self) -> Duration {
        self.scheduler
    }
}

impl FriWitnessGeneratorConfig {
    pub fn witness_generation_timeouts(&self) -> WitnessGenerationTimeouts {
        WitnessGenerationTimeouts {
            basic: self
                .basic_generation_timeout_in_secs
                .unwrap_or(self.generation_timeout_in_secs),
            leaf: self
                .leaf_generation_timeout_in_secs
                .unwrap_or(self.generation_timeout_in_secs),
            node: self
                .node_generation_timeout_in_secs
                .unwrap_or(self.generation_timeout_in_secs),
            recursion_tip: self
                .recursion_tip_generation_timeout_in_secs
                .unwrap_or(self.generation_timeout_in_secs),
            scheduler: self
                .scheduler_generation_timeout_in_secs
                .unwrap_or(self.generation_timeout_in_secs),
        }
    }
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> FriWitnessGeneratorConfig {
        FriWitnessGeneratorConfig {
            generation_timeout_in_secs: Duration::from_secs(900),
            basic_generation_timeout_in_secs: Some(Duration::from_secs(900)),
            leaf_generation_timeout_in_secs: Some(Duration::from_secs(800)),
            node_generation_timeout_in_secs: Some(Duration::from_secs(800)),
            recursion_tip_generation_timeout_in_secs: Some(Duration::from_secs(700)),
            scheduler_generation_timeout_in_secs: Some(Duration::from_secs(900)),
            max_attempts: 4,
            last_l1_batch_to_process: Some(L1BatchNumber(123456)),
            prometheus_listener_port: Some(3333),
            max_circuits_in_flight: 500,
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            FRI_WITNESS_GENERATION_TIMEOUT_IN_SECS=900
            FRI_WITNESS_BASIC_GENERATION_TIMEOUT_IN_SECS=900
            FRI_WITNESS_LEAF_GENERATION_TIMEOUT_IN_SECS=800
            FRI_WITNESS_NODE_GENERATION_TIMEOUT_IN_SECS=800
            FRI_WITNESS_RECURSION_TIP_GENERATION_TIMEOUT_IN_SECS=700
            FRI_WITNESS_SCHEDULER_GENERATION_TIMEOUT_IN_SECS=900
            FRI_WITNESS_MAX_ATTEMPTS=4
            FRI_WITNESS_LAST_L1_BATCH_TO_PROCESS=123456
            FRI_WITNESS_SHALL_SAVE_TO_PUBLIC_BUCKET=true
            FRI_WITNESS_PROMETHEUS_LISTENER_PORT=3333
            FRI_WITNESS_MAX_CIRCUITS_IN_FLIGHT=500
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("FRI_WITNESS_");

        let config: FriWitnessGeneratorConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          generation_timeout_in_secs: 900
          basic_generation_timeout_in_secs: 900
          leaf_generation_timeout_in_secs: 800
          node_generation_timeout_in_secs: 800
          scheduler_generation_timeout_in_secs: 900
          recursion_tip_timeout_in_secs: 700
          last_l1_batch_to_process: 123456
          max_attempts: 4
          shall_save_to_public_bucket: true
          prometheus_listener_port: 3333
          max_circuits_in_flight: 500
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: FriWitnessGeneratorConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
