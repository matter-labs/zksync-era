use serde::Deserialize;

use super::envy_load;

#[derive(Debug, Deserialize, Eq, PartialEq, Clone, Copy)]
pub enum ObjectStoreMode {
    GCS,
    GCSWithCredentialFile,
    FileBacked,
}

/// Configuration for the object store
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ObjectStoreConfig {
    pub bucket_base_url: String,
    pub mode: ObjectStoreMode,
    pub file_backed_base_path: String,
    pub gcs_credential_file_path: String,
    pub max_retries: u16,
}

impl ObjectStoreConfig {
    pub fn from_env() -> Self {
        envy_load("object_store", "OBJECT_STORE_")
    }

    pub fn public_from_env() -> Self {
        envy_load("public_object_store", "PUBLIC_OBJECT_STORE_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

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
        let actual = ObjectStoreConfig::from_env();
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
        let actual = ObjectStoreConfig::public_from_env();
        assert_eq!(actual, expected_config("/public_base_url"));
    }
}
