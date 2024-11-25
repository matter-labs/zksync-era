use smart_config::{DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ExternalProofIntegrationApiConfig {
    #[config(default_t = 3_073)]
    pub http_port: u16,
}
