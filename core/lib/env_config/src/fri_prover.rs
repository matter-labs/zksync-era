use zksync_config::configs::FriProverConfig;

use crate::{envy_load, object_store::ProverObjectStoreConfig, FromEnv};

impl FromEnv for FriProverConfig {
    fn from_env() -> anyhow::Result<Self> {
        let mut prover: FriProverConfig = envy_load("fri_prover", "FRI_PROVER_")?;
        prover.prover_object_store = ProverObjectStoreConfig::from_env().map(|a| a.0).ok();
        Ok(prover)
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::{configs::object_store::ObjectStoreMode, ObjectStoreConfig};

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> FriProverConfig {
        FriProverConfig {
            setup_data_path: "prover/data/keys".to_string(),
            prometheus_port: 3315,
            max_attempts: 10,
            generation_timeout_in_secs: 300,
            prover_object_store: Some(ObjectStoreConfig {
                mode: ObjectStoreMode::GCSWithCredentialFile {
                    bucket_base_url: "/base/url".to_owned(),
                    gcs_credential_file_path: "/path/to/credentials1.json".to_owned(),
                },
                max_retries: 5,
                local_mirror_path: None,
            }),
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            FRI_PROVER_SETUP_DATA_PATH="prover/data/keys"
            FRI_PROVER_PROMETHEUS_PORT="3315"
            FRI_PROVER_MAX_ATTEMPTS="10"
            FRI_PROVER_GENERATION_TIMEOUT_IN_SECS="300"
            PROVER_OBJECT_STORE_BUCKET_BASE_URL="/base/url"
            PROVER_OBJECT_STORE_MODE="GCSWithCredentialFile"
            PROVER_OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH="/path/to/credentials1.json"
        "#;
        lock.set_env(config);

        let actual = FriProverConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
