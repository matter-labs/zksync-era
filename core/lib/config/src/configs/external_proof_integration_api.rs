use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ExternalProofIntegrationApiConfig {
    pub http_port: u16,
}
