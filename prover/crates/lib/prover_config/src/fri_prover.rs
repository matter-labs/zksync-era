use std::{path::PathBuf, time::Duration};

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};
use zksync_config::ObjectStoreConfig;

/// Configuration for the fri prover application
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct FriProverConfig {
    pub setup_data_path: PathBuf,
    pub prometheus_port: Option<u16>,
    #[config(default_t = 5)]
    pub max_attempts: u32,
    #[config(default_t = 10 * TimeUnit::Minutes)]
    pub generation_timeout: Duration,
    #[config(nest)]
    pub prover_object_store: ObjectStoreConfig,
}

#[cfg(test)]
mod tests {
    use smart_config::{
        testing::{test_complete, Tester},
        Environment, Yaml,
    };
    use zksync_config::configs::object_store::ObjectStoreMode;

    use super::*;

    fn expected_config() -> FriProverConfig {
        FriProverConfig {
            setup_data_path: "prover/data/keys".into(),
            prometheus_port: Some(3315),
            max_attempts: 10,
            generation_timeout: Duration::from_secs(300),
            prover_object_store: ObjectStoreConfig {
                mode: ObjectStoreMode::GCSWithCredentialFile {
                    bucket_base_url: "/base/url".to_owned(),
                    gcs_credential_file_path: "/path/to/credentials1.json".into(),
                },
                max_retries: 5,
                local_mirror_path: Some("/var/cache".into()),
            },
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            FRI_PROVER_SETUP_DATA_PATH="prover/data/keys"
            FRI_PROVER_PROMETHEUS_PORT="3315"
            FRI_PROVER_MAX_ATTEMPTS="10"
            FRI_PROVER_GENERATION_TIMEOUT_IN_SECS="300"
            FRI_PROVER_PROVER_OBJECT_STORE_BUCKET_BASE_URL="/base/url"
            FRI_PROVER_PROVER_OBJECT_STORE_MODE="GCSWithCredentialFile"
            FRI_PROVER_PROVER_OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH="/path/to/credentials1.json"
            FRI_PROVER_PROVER_OBJECT_STORE_MAX_RETRIES="5"
            FRI_PROVER_PROVER_OBJECT_STORE_LOCAL_MIRROR_PATH="/var/cache"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("FRI_PROVER_");

        let config: FriProverConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          setup_data_path: prover/data/keys
          prometheus_port: 3315
          max_attempts: 10
          generation_timeout_in_secs: 300
          prover_object_store:
            mode: GCSWithCredentialFile
            bucket_base_url: "/base/url"
            gcs_credential_file_path: /path/to/credentials1.json
            max_retries: 5
            local_mirror_path: /var/cache
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: FriProverConfig = Tester::default()
            .coerce_variant_names()
            .test_complete(yaml)
            .unwrap();
        assert_eq!(config, expected_config());
    }
}
