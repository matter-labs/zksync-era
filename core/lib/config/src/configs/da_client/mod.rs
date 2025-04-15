use smart_config::{DescribeConfig, DeserializeConfig};

use crate::{AvailConfig, CelestiaConfig, EigenConfig, ObjectStoreConfig};

pub mod avail;
pub mod celestia;
pub mod eigen;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(tag = "type")]
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

    use smart_config::{testing::test, Yaml};

    use super::{avail::AvailClientConfig, eigen::PointsSource, *};

    #[test]
    fn no_da_config_from_yaml() {
        let yaml = "type: NoDA";
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config = test::<DAClientConfig>(yaml).unwrap();
        assert_eq!(config, DAClientConfig::NoDA);
    }

    #[test]
    fn object_store_config_from_yaml() {
        let yaml = r#"
          type: ObjectStore
          mode: FileBacked
          file_backed_base_path: ./chains/era/artifacts/
          max_retries: 10
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test::<DAClientConfig>(yaml).unwrap();
        let DAClientConfig::ObjectStore(config) = config else {
            panic!("unexpected config: {config:?}");
        };
        assert_eq!(config.max_retries, 10);
    }

    #[test]
    fn gas_relay_avail_config_from_yaml() {
        let yaml = r#"
          type: Avail
          bridge_api_url: https://bridge-api.avail.so
          timeout_ms: 20000
          avail_client: GasRelay
          gas_relay_api_url: https://lens-turbo-api.availproject.org
          max_retries: 4
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test::<DAClientConfig>(yaml).unwrap();
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

    #[test]
    fn full_avail_config_from_yaml() {
        let yaml = r#"
          type: Avail
          bridge_api_url: https://turing-bridge-api.avail.so
          timeout: 20s
          avail_client: FullClient
          api_node_url: wss://turing-rpc.avail.so/ws
          app_id: 123456
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test::<DAClientConfig>(yaml).unwrap();
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
    fn eigen_config_from_yaml() {
        let yaml = r#"
          type: Eigen
          disperser_rpc: https://disperser-holesky.eigenda.xyz:443
          settlement_layer_confirmation_depth: 1
          eigenda_eth_rpc: https://holesky.infura.io/
          eigenda_svc_manager_address: 0xD4A7E1Bd8015057293f0D0A557088c286942e84b
          wait_for_finalization: true
          authenticated: true
          points_source:
            type: Url
            g1_url: https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g1.point
            g2_url: https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g2.point.powerOf2
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test::<DAClientConfig>(yaml).unwrap();
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
        let PointsSource::Url { g1_url, g2_url } = &config.points_source else {
            panic!("Unexpected config: {config:?}");
        };
        assert_eq!(g1_url, "https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g1.point");
        assert_eq!(g2_url, "https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g2.point.powerOf2");
    }
}
