use smart_config::{DescribeConfig, DeserializeConfig};

use crate::{AvailConfig, CelestiaConfig, EigenConfig, ObjectStoreConfig};

pub mod avail;
pub mod celestia;
pub mod eigen;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(tag = "client")]
pub enum DAClientConfig {
    Avail(AvailConfig),
    Celestia(CelestiaConfig),
    Eigen(EigenConfig),
    ObjectStore(ObjectStoreConfig),
    #[config(alias = "NoDa")]
    NoDA,
}

impl From<AvailConfig> for DAClientConfig {
    fn from(config: AvailConfig) -> Self {
        Self::Avail(config)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use secrecy::ExposeSecret;
    use smart_config::{
        testing::{test, test_complete, Tester},
        ConfigRepository, ConfigSchema, Environment, Yaml,
    };

    use super::{avail::AvailClientConfig, *};
    use crate::configs::{object_store::ObjectStoreMode, DataAvailabilitySecrets, Secrets};

    #[test]
    fn no_da_config_from_yaml() {
        let yaml = "client: NoDA";
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config = test_complete::<DAClientConfig>(yaml).unwrap();
        assert_eq!(config, DAClientConfig::NoDA);
    }

    #[test]
    fn object_store_config_from_env() {
        let env = r#"
          DA_CLIENT="ObjectStore"
          DA_BUCKET_BASE_URL="some/test/path"
          DA_MODE="GCS"
          DA_MAX_RETRIES="5"
          DA_LOCAL_MIRROR_PATH="/var/cache"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("DA_");

        let config = test_complete::<DAClientConfig>(env).unwrap();
        let DAClientConfig::ObjectStore(config) = config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(config.max_retries, 5);
        let ObjectStoreMode::GCS { bucket_base_url } = &config.mode else {
            panic!("Unexpected config: {config:?}");
        };
        assert_eq!(bucket_base_url, "some/test/path");
    }

    #[test]
    fn object_store_config_from_yaml() {
        let yaml = r#"
          client: ObjectStore
          mode: FileBacked
          file_backed_base_path: ./chains/era/artifacts/
          max_retries: 10
          local_mirror_path: /var/cache
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test_complete::<DAClientConfig>(yaml).unwrap();
        let DAClientConfig::ObjectStore(config) = config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(config.max_retries, 10);
    }

    #[test]
    fn object_store_config_from_yaml_with_enum_coercion() {
        let yaml = r#"
          object_store:
            file_backed:
              file_backed_base_path: ./chains/era/artifacts/
            max_retries: 10
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = Tester::<DAClientConfig>::default()
            .coerce_serde_enums()
            .test(yaml)
            .unwrap();
        let DAClientConfig::ObjectStore(config) = config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(config.max_retries, 10);
    }

    #[test]
    fn no_da_config_from_yaml_with_enum_coercion() {
        let yaml = r#"
            no_da: {}
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = Tester::<DAClientConfig>::default()
            .coerce_serde_enums()
            .test(yaml)
            .unwrap();
        assert_eq!(config, DAClientConfig::NoDA);
    }

    #[test]
    fn avail_config_from_env() {
        let env = r#"
          DA_CLIENT="Avail"
          DA_AVAIL_CLIENT_TYPE="FullClient"
          DA_BRIDGE_API_URL="localhost:54321"
          DA_TIMEOUT_MS="2000"
          DA_API_NODE_URL="localhost:12345"
          DA_APP_ID="1"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("DA_");

        let config = test::<DAClientConfig>(env).unwrap();
        let DAClientConfig::Avail(config) = config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(config.bridge_api_url, "localhost:54321");
        assert_eq!(config.timeout, Duration::from_secs(2));
        let AvailClientConfig::FullClient(client) = config.config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(client.app_id, 1);
        assert_eq!(client.api_node_url, "localhost:12345");
    }

    #[test]
    fn gas_relay_avail_config_from_yaml() {
        let yaml = r#"
          client: Avail
          bridge_api_url: https://bridge-api.avail.so
          timeout_ms: 20000
          avail_client_type: GasRelay
          gas_relay_api_url: https://lens-turbo-api.availproject.org
          max_retries: 4
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test_complete::<DAClientConfig>(yaml).unwrap();
        assert_gas_relay_avail_config(&config);
    }

    fn assert_gas_relay_avail_config(config: &DAClientConfig) {
        let DAClientConfig::Avail(config) = config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(config.bridge_api_url, "https://bridge-api.avail.so");
        assert_eq!(config.timeout, Duration::from_secs(20));
        let AvailClientConfig::GasRelay(client) = &config.config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(
            client.gas_relay_api_url,
            "https://lens-turbo-api.availproject.org"
        );
        assert_eq!(client.max_retries, 4);
    }

    #[test]
    fn gas_relay_avail_config_from_yaml_with_enum_coercion() {
        let yaml = r#"
          avail:
            bridge_api_url: https://bridge-api.avail.so
            timeout_ms: 20000
            gas_relay:
              gas_relay_api_url: https://lens-turbo-api.availproject.org
              max_retries: 4
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = Tester::<DAClientConfig>::default()
            .coerce_serde_enums()
            .test(yaml)
            .unwrap();
        assert_gas_relay_avail_config(&config);
    }

    // Checks that non-secret and secret parts of the DA config don't clash despite previously having differing prefixes
    // (`da_client` vs `da`).
    #[test]
    fn secrets_do_not_clash_with_client_tag() {
        let yaml = r#"
          da_client:
            client: Avail
            bridge_api_url: https://bridge-api.avail.so
            avail_client_type: GasRelay
            gas_relay_api_url: https://lens-turbo-api.availproject.org
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let mut schema = ConfigSchema::new(&DAClientConfig::DESCRIPTION, "da_client");
        schema.insert(&Secrets::DESCRIPTION, "").unwrap();

        let repo = ConfigRepository::new(&schema).with(yaml);
        let config: DAClientConfig = repo.single().unwrap().parse().unwrap();
        assert!(matches!(&config, DAClientConfig::Avail(_)));
        let secrets: DataAvailabilitySecrets = repo.single().unwrap().parse().unwrap();
        assert!(matches!(&secrets, DataAvailabilitySecrets::Avail(_)));

        let secrets = r#"
          da:
            client: Avail
            gas_relay_api_key: SUPER_SECRET_KEY
        "#;
        let secrets = Yaml::new("secrets.yml", serde_yaml::from_str(secrets).unwrap()).unwrap();
        let repo = repo.with(secrets);

        let config: DAClientConfig = repo.single().unwrap().parse().unwrap();
        assert!(matches!(&config, DAClientConfig::Avail(_)));
        let secrets: DataAvailabilitySecrets = repo.single().unwrap().parse().unwrap();
        let DataAvailabilitySecrets::Avail(secrets) = secrets else {
            panic!("unexpected secrets: {secrets:?}");
        };
        assert_eq!(
            secrets.gas_relay_api_key.unwrap().0.expose_secret(),
            "SUPER_SECRET_KEY"
        );
    }

    #[test]
    fn full_avail_config_from_yaml() {
        let yaml = r#"
          client: Avail
          bridge_api_url: https://turing-bridge-api.avail.so
          timeout: 20s
          dispatch_timeout: 5s
          avail_client_type: FullClient
          api_node_url: wss://turing-rpc.avail.so/ws
          app_id: 123456
          max_blocks_to_look_back: 5
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test_complete::<DAClientConfig>(yaml).unwrap();
        assert_full_avail_config(&config);
    }

    fn assert_full_avail_config(config: &DAClientConfig) {
        let DAClientConfig::Avail(config) = config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(config.bridge_api_url, "https://turing-bridge-api.avail.so");
        assert_eq!(config.timeout, Duration::from_secs(20));
        let AvailClientConfig::FullClient(client) = &config.config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(client.api_node_url, "wss://turing-rpc.avail.so/ws");
        assert_eq!(client.app_id, 123_456);
    }

    #[test]
    fn full_avail_config_from_yaml_with_enum_coercion() {
        let yaml = r#"
          avail:
            bridge_api_url: https://turing-bridge-api.avail.so
            timeout: 20s
            full_client:
              api_node_url: wss://turing-rpc.avail.so/ws
              app_id: 123456
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = Tester::<DAClientConfig>::default()
            .coerce_serde_enums()
            .test(yaml)
            .unwrap();
        assert_full_avail_config(&config);
    }

    #[test]
    fn eigen_config_from_env() {
        let env = r#"
          DA_CLIENT="Eigen"
          DA_DISPERSER_RPC="http://localhost:8080"
          DA_EIGENDA_ETH_RPC="http://localhost:8545"
          DA_CERT_VERIFIER_ROUTER_ADDR="0x0000000000000000000000000000000000000123"
          DA_OPERATOR_STATE_RETRIEVER_ADDR="0x0000000000000000000000000000000000000124"
          DA_REGISTRY_COORDINATOR_ADDR="0x0000000000000000000000000000000000000125"
          DA_BLOB_VERSION="0"
          DA_EIGENDA_PROVER_SERVICE_RPC="http://localhost:9999"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("DA_");

        let config = test_complete::<DAClientConfig>(env).unwrap();
        let DAClientConfig::Eigen(config) = config else {
            panic!("unexpected config: {config:?}");
        };

        assert_eq!(config.disperser_rpc, "http://localhost:8080");
        assert_eq!(
            config.eigenda_eth_rpc.as_ref().unwrap().expose_str(),
            "http://localhost:8545/"
        );

        assert_eq!(config.blob_version, 0);
        assert_eq!(
            config.cert_verifier_router_addr,
            "0x0000000000000000000000000000000000000123"
        );
        assert_eq!(
            config.operator_state_retriever_addr,
            "0x0000000000000000000000000000000000000124"
        );
        assert_eq!(
            config.registry_coordinator_addr,
            "0x0000000000000000000000000000000000000125"
        );

        assert_eq!(
            config.eigenda_prover_service_rpc,
            Some("http://localhost:9999".to_string())
        );
    }

    #[test]
    fn eigen_config_from_yaml() {
        let yaml = r#"
            client: Eigen
            disperser_rpc: https://disperser-holesky.eigenda.xyz:443
            eigenda_eth_rpc: https://holesky.infura.io/
            cert_verifier_router_addr: "0x0000000000000000000000000000000000000123"
            operator_state_retriever_addr: "0x0000000000000000000000000000000000000124"
            registry_coordinator_addr: "0x0000000000000000000000000000000000000125"
            blob_version: 0
            eigenda_prover_service_rpc: http://localhost:9999
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test_complete::<DAClientConfig>(yaml).unwrap();
        assert_eigen_config(&config);
    }

    fn assert_eigen_config(config: &DAClientConfig) {
        let DAClientConfig::Eigen(config) = config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(
            config.disperser_rpc,
            "https://disperser-holesky.eigenda.xyz:443"
        );
        assert_eq!(
            config.eigenda_eth_rpc.as_ref().unwrap().expose_str(),
            "https://holesky.infura.io/"
        );

        assert_eq!(config.blob_version, 0);
        assert_eq!(
            config.cert_verifier_router_addr,
            "0x0000000000000000000000000000000000000123"
        );
        assert_eq!(
            config.operator_state_retriever_addr,
            "0x0000000000000000000000000000000000000124"
        );
        assert_eq!(
            config.registry_coordinator_addr,
            "0x0000000000000000000000000000000000000125"
        );

        assert_eq!(
            config.eigenda_prover_service_rpc,
            Some("http://localhost:9999".to_string())
        );
    }

    #[test]
    fn eigen_config_from_yaml_with_enum_coercion() {
        let yaml = r#"
          eigen:
            disperser_rpc: https://disperser-holesky.eigenda.xyz:443
            eigenda_eth_rpc: https://holesky.infura.io/
            blob_version: 0
            cert_verifier_router_addr: "0x0000000000000000000000000000000000000123"
            operator_state_retriever_addr: "0x0000000000000000000000000000000000000124"
            registry_coordinator_addr: "0x0000000000000000000000000000000000000125"
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = Tester::<DAClientConfig>::default()
            .coerce_serde_enums()
            .test(yaml)
            .unwrap();
        assert_eigen_config(&config);
    }
}
