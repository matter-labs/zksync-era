use std::{path::PathBuf, time::Duration};

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

use crate::ObjectStoreConfig;

/// Configuration for the fri prover application
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct FriProverConfig {
    pub setup_data_path: PathBuf,
    #[config(default_t = 3315)]
    pub prometheus_port: u16,
    #[config(default_t = 5)]
    pub max_attempts: u32,
    #[config(default_t = Duration::from_secs(600), with = TimeUnit::Seconds)]
    pub generation_timeout_in_secs: Duration,
    #[config(nest)]
    pub prover_object_store: ObjectStoreConfig,
}

#[cfg(test)]
mod tests {
    use smart_config::{
        testing::{test, Tester},
        Environment, Yaml,
    };

    use super::*;
    use crate::configs::object_store::ObjectStoreMode;

    fn expected_config() -> FriProverConfig {
        FriProverConfig {
            setup_data_path: "prover/data/keys".into(),
            prometheus_port: 3315,
            max_attempts: 10,
            generation_timeout_in_secs: Duration::from_secs(300),
            prover_object_store: ObjectStoreConfig {
                mode: ObjectStoreMode::GCSWithCredentialFile {
                    bucket_base_url: "/base/url".to_owned(),
                    gcs_credential_file_path: "/path/to/credentials1.json".to_owned(),
                },
                max_retries: 5,
                local_mirror_path: None,
            },
        }
    }

    // TODO: test with non-prefixed `PROVER_OBJECT_STORE_` / `PUBLIC_OBJECT_STORE_` (requires global aliases)
    #[test]
    fn parsing_from_env() {
        let env = r#"
            FRI_PROVER_SETUP_DATA_PATH="prover/data/keys"
            FRI_PROVER_PROMETHEUS_PORT="3315"
            FRI_PROVER_MAX_ATTEMPTS="10"
            FRI_PROVER_GENERATION_TIMEOUT_IN_SECS="300"
            FRI_PROVER_SETUP_LOAD_MODE="FromDisk"
            FRI_PROVER_SPECIALIZED_GROUP_ID="10"
            FRI_PROVER_QUEUE_CAPACITY="10"
            FRI_PROVER_WITNESS_VECTOR_RECEIVER_PORT="3316"
            FRI_PROVER_ZONE_READ_URL="http://metadata.google.internal/computeMetadata/v1/instance/zone"
            FRI_PROVER_SHALL_SAVE_TO_PUBLIC_BUCKET=true
            FRI_PROVER_AVAILABILITY_CHECK_INTERVAL_IN_SECS="1800"
            FRI_PROVER_CLOUD_TYPE=GCP
            FRI_PROVER_PROVER_OBJECT_STORE_BUCKET_BASE_URL="/base/url"
            FRI_PROVER_PROVER_OBJECT_STORE_MODE="GCSWithCredentialFile"
            FRI_PROVER_PROVER_OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH="/path/to/credentials1.json"
            FRI_PROVER_PROVER_OBJECT_STORE_MAX_RETRIES="5"
            FRI_PROVER_PUBLIC_OBJECT_STORE_BUCKET_BASE_URL="/base/url"
            FRI_PROVER_PUBLIC_OBJECT_STORE_MODE="FileBacked"
            FRI_PROVER_PUBLIC_OBJECT_STORE_FILE_BACKED_BASE_PATH="./chains/era/artifacts/"
            FRI_PROVER_PUBLIC_OBJECT_STORE_LOCAL_MIRROR_PATH="/tmp/mirror"
            FRI_PROVER_PUBLIC_OBJECT_STORE_MAX_RETRIES="5"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("FRI_PROVER_");

        let config: FriProverConfig = test(env).unwrap();
        assert_eq!(config, expected_config());
    }

    // Migration path: use tagged enums for object stores
    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          setup_data_path: prover/data/keys
          prometheus_port: 3315
          max_attempts: 10
          generation_timeout_in_secs: 300
          setup_load_mode: FROM_DISK
          specialized_group_id: 10
          queue_capacity: 10
          witness_vector_receiver_port: 3316
          zone_read_url: http://metadata.google.internal/computeMetadata/v1/instance/zone
          shall_save_to_public_bucket: true
          availability_check_interval_in_secs: 1800
          prover_object_store:
            mode: GCSWithCredentialFile
            bucket_base_url: "/base/url"
            gcs_credential_file_path: /path/to/credentials1.json
            max_retries: 5
            local_mirror_path: null
          public_object_store:
            mode: FileBacked
            file_backed_base_path: ./chains/era/artifacts/
            max_retries: 5
            local_mirror_path: /tmp/mirror
          cloud_type: GCP
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: FriProverConfig = Tester::default().coerce_variant_names().test(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
