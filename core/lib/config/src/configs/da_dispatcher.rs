use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct DADispatcherConfig {
    /// The interval between the `da_dispatcher's` iterations.
    #[config(default_t = Duration::from_secs(5), with = TimeUnit::Millis)]
    pub polling_interval_ms: Duration,
    /// The maximum number of rows to query from the database in a single query.
    #[config(default_t = 100)]
    pub max_rows_to_dispatch: u32,
    /// The maximum number of retries for the dispatch of a blob.
    #[config(default_t = 5)]
    pub max_retries: u16,
    /// Use dummy value as inclusion proof instead of getting it from the client.
    // TODO: run a verification task to check if the L1 contract expects the inclusion proofs to
    //   avoid the scenario where contracts expect real proofs, and server is using dummy proofs.
    #[config(default)]
    pub use_dummy_inclusion_data: bool,
    /// This flag is used to signal that the transition to the full DA validators is in progress.
    /// It will make the dispatcher stop polling for inclusion data and ensure all the old batches
    /// have at least dummy inclusion data.
    #[config(default)]
    pub inclusion_verification_transition_enabled: bool,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> DADispatcherConfig {
        DADispatcherConfig {
            polling_interval_ms: Duration::from_secs(5),
            max_rows_to_dispatch: 60,
            max_retries: 7,
            use_dummy_inclusion_data: true,
            inclusion_verification_transition_enabled: false,
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            DA_DISPATCHER_POLLING_INTERVAL_MS=5000
            DA_DISPATCHER_MAX_ROWS_TO_DISPATCH=60
            DA_DISPATCHER_MAX_RETRIES=7
            DA_DISPATCHER_USE_DUMMY_INCLUSION_DATA="true"
            DA_DISPATCHER_INCLUSION_VERIFICATION_TRANSITION_ENABLED="false"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("DA_DISPATCHER_");

        let config: DADispatcherConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          polling_interval_ms: 5000
          max_rows_to_dispatch: 60
          max_retries: 7
          use_dummy_inclusion_data: true
          inclusion_verification_transition_enabled: false
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: DADispatcherConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
