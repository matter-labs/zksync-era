use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProverApiConfig {
    pub http_port: u16,
    pub last_available_batch: u32,
}
