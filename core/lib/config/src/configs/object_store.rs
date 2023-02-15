use crate::envy_load;
use serde::Deserialize;

/// Configuration for the object store
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ObjectStoreConfig {
    pub service_account_path: String,
    pub bucket_base_url: String,
    pub mode: String,
    pub file_backed_base_path: String,
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
            service_account_path: "/path/to/service_account.json".to_string(),
            bucket_base_url: "/base/url".to_string(),
            mode: "FileBacked".to_string(),
            file_backed_base_path: "artifacts".to_string(),
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
OBJECT_STORE_SERVICE_ACCOUNT_PATH="/path/to/service_account.json"
OBJECT_STORE_BUCKET_BASE_URL="/base/url"
OBJECT_STORE_MODE="FileBacked"
OBJECT_STORE_FILE_BACKED_BASE_PATH="artifacts"
        "#;
        set_env(config);
        let actual = ObjectStoreConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
