use std::{
    num::{NonZeroU64, NonZeroUsize},
    time::Duration,
};

use smart_config::{
    de::{Optional, Serde},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::{url::SensitiveUrl, L1ChainId, L2ChainId, SLChainId};

/// Temporary config for initializing external node, will be completely replaced by consensus config later
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ENConfig {
    // Genesis
    #[config(with = Serde![int])]
    pub l2_chain_id: L2ChainId,
    #[config(with = Serde![int])]
    pub l1_chain_id: L1ChainId,

    // Main node configuration
    #[config(secret, with = Serde![str])]
    pub main_node_url: SensitiveUrl,
    #[config(default_t = NonZeroUsize::new(100).unwrap())]
    pub main_node_rate_limit_rps: NonZeroUsize,
    #[config(secret, with = Optional(Serde![str]))]
    pub gateway_url: Option<SensitiveUrl>,
    pub bridge_addresses_refresh_interval: Option<Duration>,
    #[config(with = Optional(Serde![int]))]
    pub gateway_chain_id: Option<SLChainId>,
    pub batch_transaction_updater_interval: Option<Duration>,
    #[config(with = Optional(Serde![int]))]
    pub batch_transaction_updater_batch_size: Option<NonZeroU64>,
}

#[cfg(test)]
mod tests {
    use smart_config::{ConfigRepository, ConfigSchema, Environment, Yaml};

    use super::*;

    fn expected_config() -> ENConfig {
        ENConfig {
            l2_chain_id: L2ChainId::from(271),
            l1_chain_id: L1ChainId(9),
            gateway_chain_id: Some(SLChainId(123)),
            main_node_url: "http://127.0.0.1:3050/".parse().unwrap(),
            main_node_rate_limit_rps: NonZeroUsize::new(200).unwrap(),
            gateway_url: None,
            bridge_addresses_refresh_interval: Some(Duration::from_secs(15)),
            batch_transaction_updater_interval: Some(Duration::from_secs(7)),
            batch_transaction_updater_batch_size: NonZeroU64::new(10),
        }
    }

    fn create_schema() -> ConfigSchema {
        let mut schema = ConfigSchema::default();
        schema
            .insert(&ENConfig::DESCRIPTION, "external_node")
            .unwrap()
            .push_alias("")
            .unwrap();
        schema
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            EN_L1_CHAIN_ID=9
            EN_L2_CHAIN_ID=271
            EN_GATEWAY_CHAIN_ID=123
            EN_MAIN_NODE_URL=http://127.0.0.1:3050/
            EN_MAIN_NODE_RATE_LIMIT_RPS=200
            EN_BRIDGE_ADDRESSES_REFRESH_INTERVAL="15s"
            EN_BATCH_TRANSACTION_UPDATER_INTERVAL="7s"
            EN_BATCH_TRANSACTION_UPDATER_BATCH_SIZE=10
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("EN_");

        let schema = create_schema();
        let repo = ConfigRepository::new(&schema).with(env);
        let config: ENConfig = repo.single().unwrap().parse().unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
            main_node_url: http://127.0.0.1:3050/
            main_node_rate_limit_rps: 200
            gateway_url: null
            l2_chain_id: 271
            l1_chain_id: 9
            gateway_chain_id: 123
            bridge_addresses_refresh_interval: '15s'
            batch_transaction_updater_interval: '7s'
            batch_transaction_updater_batch_size: 10
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let schema = create_schema();
        let repo = ConfigRepository::new(&schema).with(yaml);
        let config: ENConfig = repo.single().unwrap().parse().unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_canonical_yaml() {
        let yaml = r#"
          external_node:
            main_node_url: http://127.0.0.1:3050/
            main_node_rate_limit_rps: 200
            gateway_url: null
            l2_chain_id: 271
            l1_chain_id: 9
            gateway_chain_id: 123
            bridge_addresses_refresh_interval: '15s'
            batch_transaction_updater_interval: '7s'
            batch_transaction_updater_batch_size: 10
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let schema = create_schema();
        let repo = ConfigRepository::new(&schema).with(yaml);
        let config: ENConfig = repo.single().unwrap().parse().unwrap();
        assert_eq!(config, expected_config());
    }
}
