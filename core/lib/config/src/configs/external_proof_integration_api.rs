use smart_config::{DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ExternalProofIntegrationApiConfig {
    #[config(default_t = 3_073)]
    pub http_port: u16,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> ExternalProofIntegrationApiConfig {
        ExternalProofIntegrationApiConfig { http_port: 3320 }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            EXTERNAL_PROOF_INTEGRATION_API_HTTP_PORT="3320"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("EXTERNAL_PROOF_INTEGRATION_API_");

        let config: ExternalProofIntegrationApiConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          http_port: 3320
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ExternalProofIntegrationApiConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
