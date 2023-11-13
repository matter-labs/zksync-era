use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct AlertsConfig {
    /// List of panics' messages from external crypto code,
    /// that are sporadic and needed to be handled separately
    pub sporadic_crypto_errors_substrs: Vec<String>,
}
