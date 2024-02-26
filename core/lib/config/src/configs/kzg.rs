use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct KzgConfig {
    /// Path to kzg trusted setup file.
    #[serde(default = "KzgConfig::default_trusted_setup_path")]
    pub trusted_setup_path: String,
}

impl KzgConfig {
    fn default_trusted_setup_path() -> String {
        "./trusted_setup.json".to_owned()
    }
}
