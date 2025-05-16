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
        testing::{test, test_complete},
        ConfigRepository, ConfigSchema, Environment, Yaml,
    };

    use super::{avail::AvailClientConfig, eigen::PointsSource, *};
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
        let DAClientConfig::Avail(config) = config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(config.bridge_api_url, "https://bridge-api.avail.so");
        assert_eq!(config.timeout, Duration::from_secs(20));
        let AvailClientConfig::GasRelay(client) = config.config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(
            client.gas_relay_api_url,
            "https://lens-turbo-api.availproject.org"
        );
        assert_eq!(client.max_retries, 4);
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
          finality_state: inBlock
          avail_client_type: FullClient
          api_node_url: wss://turing-rpc.avail.so/ws
          app_id: 123456
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test_complete::<DAClientConfig>(yaml).unwrap();
        let DAClientConfig::Avail(config) = config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(config.bridge_api_url, "https://turing-bridge-api.avail.so");
        assert_eq!(config.timeout, Duration::from_secs(20));
        let AvailClientConfig::FullClient(client) = config.config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(client.api_node_url, "wss://turing-rpc.avail.so/ws");
        assert_eq!(client.app_id, 123_456);
    }

    #[test]
    fn eigen_config_from_env() {
        let env = r#"
          DA_CLIENT="Eigen"
          DA_DISPERSER_RPC="http://localhost:8080"
          DA_SETTLEMENT_LAYER_CONFIRMATION_DEPTH=0
          DA_EIGENDA_ETH_RPC="http://localhost:8545"
          DA_EIGENDA_SVC_MANAGER_ADDRESS="0x0000000000000000000000000000000000000123"
          DA_WAIT_FOR_FINALIZATION=true
          DA_AUTHENTICATED=false
          DA_POINTS_SOURCE="Path"
          DA_POINTS_PATH="./resources"
          DA_CUSTOM_QUORUM_NUMBERS="2,3"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("DA_");

        let config = test_complete::<DAClientConfig>(env).unwrap();
        let DAClientConfig::Eigen(config) = config else {
            panic!("unexpected config: {config:?}");
        };

        assert_eq!(config.disperser_rpc, "http://localhost:8080");
        assert_eq!(config.settlement_layer_confirmation_depth, 0);
        assert_eq!(
            config.eigenda_eth_rpc.as_ref().unwrap().expose_str(),
            "http://localhost:8545/"
        );
        assert!(config.wait_for_finalization);
        assert!(!config.authenticated);

        let PointsSource::Path { path } = &config.points else {
            panic!("Unexpected config: {config:?}");
        };
        assert_eq!(path, "./resources");
        assert_eq!(config.custom_quorum_numbers, [2, 3]);
    }

    #[test]
    fn eigen_config_from_yaml() {
        let yaml = r#"
          client: Eigen
          disperser_rpc: https://disperser-holesky.eigenda.xyz:443
          settlement_layer_confirmation_depth: 1
          eigenda_eth_rpc: https://holesky.infura.io/
          eigenda_svc_manager_address: 0xD4A7E1Bd8015057293f0D0A557088c286942e84b
          wait_for_finalization: true
          authenticated: true
          points:
            source: Url
            g1_url: https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g1.point
            g2_url: https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g2.point.powerOf2
          custom_quorum_numbers:
            - 1
            - 3
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test_complete::<DAClientConfig>(yaml).unwrap();
        let DAClientConfig::Eigen(config) = config else {
            panic!("unexpected config: {config:?}");
        };

        assert_eq!(
            config.disperser_rpc,
            "https://disperser-holesky.eigenda.xyz:443"
        );
        assert_eq!(config.settlement_layer_confirmation_depth, 1);
        assert_eq!(
            config.eigenda_eth_rpc.as_ref().unwrap().expose_str(),
            "https://holesky.infura.io/"
        );
        assert_eq!(
            config.eigenda_svc_manager_address,
            "0xD4A7E1Bd8015057293f0D0A557088c286942e84b"
                .parse()
                .unwrap()
        );
        assert!(config.wait_for_finalization);
        assert!(config.authenticated);
        let PointsSource::Url { g1_url, g2_url } = &config.points else {
            panic!("Unexpected config: {config:?}");
        };
        assert_eq!(g1_url, "https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g1.point");
        assert_eq!(g2_url, "https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g2.point.powerOf2");
    }
}
