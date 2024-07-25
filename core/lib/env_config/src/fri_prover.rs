use zksync_config::configs::FriProverConfig;

use crate::{
    envy_load,
    object_store::{ProverObjectStoreConfig, PublicObjectStoreConfig},
    FromEnv,
};

impl FromEnv for FriProverConfig {
    fn from_env() -> anyhow::Result<Self> {
        let mut prover: FriProverConfig = envy_load("fri_prover", "FRI_PROVER_")?;
        prover.prover_object_store = ProverObjectStoreConfig::from_env().map(|a| a.0).ok();
        prover.public_object_store = PublicObjectStoreConfig::from_env().map(|a| a.0).ok();
        Ok(prover)
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::{
        configs::{
            fri_prover::{CloudType, SetupLoadMode},
            object_store::ObjectStoreMode,
        },
        ObjectStoreConfig,
    };

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> FriProverConfig {
        FriProverConfig {
            setup_data_path: "vk_setup_data_generator_server_fri/data".to_string(),
            prometheus_port: 3315,
            max_attempts: 10,
            generation_timeout_in_secs: 300,
            setup_load_mode: SetupLoadMode::FromDisk,
            specialized_group_id: 10,
            queue_capacity: 10,
            witness_vector_receiver_port: 3316,
            zone_read_url: "http://metadata.google.internal/computeMetadata/v1/instance/zone"
                .to_string(),
            shall_save_to_public_bucket: true,
            prover_object_store: Some(ObjectStoreConfig {
                mode: ObjectStoreMode::GCSWithCredentialFile {
                    bucket_base_url: "/base/url".to_owned(),
                    gcs_credential_file_path: "/path/to/credentials1.json".to_owned(),
                },
                max_retries: 5,
                local_mirror_path: None,
            }),
            public_object_store: Some(ObjectStoreConfig {
                mode: ObjectStoreMode::GCSWithCredentialFile {
                    bucket_base_url: "/base/url".to_owned(),
                    gcs_credential_file_path: "/path/to/credentials2.json".to_owned(),
                },
                max_retries: 5,
                local_mirror_path: None,
            }),
            availability_check_interval_in_secs: Some(1_800),
            cloud_type: CloudType::GCP,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            FRI_PROVER_SETUP_DATA_PATH="vk_setup_data_generator_server_fri/data"
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
            PROVER_OBJECT_STORE_BUCKET_BASE_URL="/base/url"
            PROVER_OBJECT_STORE_MODE="GCSWithCredentialFile"
            PROVER_OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH="/path/to/credentials1.json"
            PROVER_OBJECT_STORE_MAX_RETRIES="5"
            PUBLIC_OBJECT_STORE_BUCKET_BASE_URL="/base/url"
            PUBLIC_OBJECT_STORE_MODE="GCSWithCredentialFile"
            PUBLIC_OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH="/path/to/credentials2.json"
            PUBLIC_OBJECT_STORE_MAX_RETRIES="5"
        "#;
        lock.set_env(config);

        let actual = FriProverConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
