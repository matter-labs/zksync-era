use crate::envy_load;
use serde::Deserialize;

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
        envy_load!("object_store", "OBJECT_STORE_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::set_env;

    fn expected_config() -> ObjectStoreConfig {
        ObjectStoreConfig {
            bucket_base_url: "/base/url".to_string(),
            mode: ObjectStoreMode::FileBacked,
            file_backed_base_path: "artifacts".to_string(),
            gcs_credential_file_path: "/path/to/credentials.json".to_string(),
            max_retries: 5,
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
OBJECT_STORE_BUCKET_BASE_URL="/base/url"
OBJECT_STORE_MODE="FileBacked"
OBJECT_STORE_FILE_BACKED_BASE_PATH="artifacts"
OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH="/path/to/credentials.json"
OBJECT_STORE_MAX_RETRIES="5"
        "#;
        set_env(config);
        let actual = ObjectStoreConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
