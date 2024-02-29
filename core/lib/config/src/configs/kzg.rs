use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct KzgConfig {
    /// Path to KZG trusted setup file.
    #[serde(default = "KzgConfig::default_trusted_setup_path")]
    pub trusted_setup_path: String,
}

impl KzgConfig {
    fn default_trusted_setup_path() -> String {
        "./trusted_setup.json".to_owned()
    }

    pub fn for_tests() -> Self {
        let zksync_home = std::env::var("ZKSYNC_HOME").unwrap();
        Self {
            trusted_setup_path: Path::new(&zksync_home)
                .join("trusted_setup.json")
                .to_str()
                .unwrap()
                .to_owned(),
        }
    }
}
