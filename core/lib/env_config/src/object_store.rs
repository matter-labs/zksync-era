use zksync_config::ObjectStoreConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ObjectStoreConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("object_store", "OBJECT_STORE_")
    }
}

/// Wrapper for `ObjectStoreConfig` that allows loading object store config using `PUBLIC_` prefix.
#[derive(Debug)]
pub struct PublicObjectStoreConfig(pub ObjectStoreConfig);

impl FromEnv for PublicObjectStoreConfig {
    fn from_env() -> anyhow::Result<Self> {
        let config = envy_load("public_object_store", "PUBLIC_OBJECT_STORE_")?;
        Ok(Self(config))
    }
}

/// Wrapper for `ObjectStoreConfig` that allows loading object store config using `PROVER_` prefix.
#[derive(Debug)]
pub struct ProverObjectStoreConfig(pub ObjectStoreConfig);

impl FromEnv for ProverObjectStoreConfig {
    fn from_env() -> anyhow::Result<Self> {
        let config = envy_load("prover_object_store", "PROVER_OBJECT_STORE_")?;
        Ok(Self(config))
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::{configs::object_store::ObjectStoreMode, ObjectStoreConfig};

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config(bucket_base_url: &str) -> ObjectStoreConfig {
        ObjectStoreConfig {
            bucket_base_url: bucket_base_url.to_string(),
            mode: ObjectStoreMode::FileBacked,
            file_backed_base_path: "artifacts".to_string(),
            gcs_credential_file_path: "/path/to/credentials.json".to_string(),
            max_retries: 5,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            OBJECT_STORE_BUCKET_BASE_URL="/base/url"
            OBJECT_STORE_MODE="FileBacked"
            OBJECT_STORE_FILE_BACKED_BASE_PATH="artifacts"
            OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH="/path/to/credentials.json"
            OBJECT_STORE_MAX_RETRIES="5"
        "#;
        lock.set_env(config);
        let actual = ObjectStoreConfig::from_env().unwrap();
        assert_eq!(actual, expected_config("/base/url"));
    }

    #[test]
    fn public_bucket_config_from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            PUBLIC_OBJECT_STORE_BUCKET_BASE_URL="/public_base_url"
            PUBLIC_OBJECT_STORE_MODE="FileBacked"
            PUBLIC_OBJECT_STORE_FILE_BACKED_BASE_PATH="artifacts"
            PUBLIC_OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH="/path/to/credentials.json"
            PUBLIC_OBJECT_STORE_MAX_RETRIES="5"
        "#;
        lock.set_env(config);
        let actual = PublicObjectStoreConfig::from_env().unwrap().0;
        assert_eq!(actual, expected_config("/public_base_url"));
    }

    #[test]
    fn prover_bucket_config_from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            PROVER_OBJECT_STORE_BUCKET_BASE_URL="/prover_base_url"
            PROVER_OBJECT_STORE_MODE="FileBacked"
            PROVER_OBJECT_STORE_FILE_BACKED_BASE_PATH="artifacts"
            PROVER_OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH="/path/to/credentials.json"
            PROVER_OBJECT_STORE_MAX_RETRIES="5"
        "#;
        lock.set_env(config);
        let actual = ProverObjectStoreConfig::from_env().unwrap().0;
        assert_eq!(actual, expected_config("/prover_base_url"));
    }
}
