use super::envy_load;
use serde::Deserialize;
/// Configuration for the Network file system.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct NfsConfig {
    pub setup_key_mount_path: String,
}

impl NfsConfig {
    pub fn from_env() -> Self {
        envy_load("nfs", "NFS_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::set_env;

    fn expected_config() -> NfsConfig {
        NfsConfig {
            setup_key_mount_path: "/path/to/setup_keys".to_string(),
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
NFS_SETUP_KEY_MOUNT_PATH="/path/to/setup_keys"
        "#;
        set_env(config);
        let actual = NfsConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
