// TODO: remove non-secret / secret config split (isn't necessary)

use smart_config::{de::Serde, DescribeConfig, DeserializeConfig};
use zksync_basic_types::url::SensitiveUrl;

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct L1Secrets {
    /// RPC URL for L1.
    #[config(alias = "eth_client_url", secret, with = Serde![str])]
    #[config(example = Some("https://ethereum-rpc.publicnode.com/".parse().unwrap()))]
    pub l1_rpc_url: Option<SensitiveUrl>,
    /// Web3 RPC URL for the gateway layer.
    #[config(secret, with = Serde![str])]
    #[config(alias = "gateway_web3_url", alias = "gateway_url")]
    pub gateway_rpc_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub struct Secrets {
    #[config(nest)]
    pub l1: L1Secrets,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn assert_secrets(secrets: Secrets) {
        assert_eq!(
            secrets.l1.l1_rpc_url.unwrap().expose_str(),
            "http://127.0.0.1:8545/"
        );
        assert_eq!(
            secrets.l1.gateway_rpc_url.unwrap().expose_str(),
            "http://127.0.0.1:4050/"
        );
    }

    // Migration path: change `DA_SECRETS_*` -> `DA_*`
    #[test]
    fn parsing_from_env() {
        let env = r#"
            # Was `ETH_CLIENT_WEB3_URL`
            L1_ETH_CLIENT_URL=http://127.0.0.1:8545/
            # Was `ETH_CLIENT_GATEWAY_WEB3_URL`
            L1_GATEWAY_WEB3_URL=http://127.0.0.1:4050/
        "#;
        let env = Environment::from_dotenv("test.env", env).unwrap();
        let secrets: Secrets = test_complete(env).unwrap();
        assert_secrets(secrets);
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
            l1:
              l1_rpc_url: http://127.0.0.1:8545/
              gateway_rpc_url: http://127.0.0.1:4050/
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let secrets: Secrets = test_complete(yaml).unwrap();
        assert_secrets(secrets);
    }
}
