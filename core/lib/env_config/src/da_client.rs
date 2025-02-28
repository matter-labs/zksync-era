use std::{env, str::FromStr};

use anyhow::Context;
use zksync_basic_types::{url::SensitiveUrl, H160};
use zksync_config::{
    configs::{
        da_client::{
            avail::{
                AvailClientConfig, AvailSecrets, AVAIL_FULL_CLIENT_NAME,
                AVAIL_GAS_RELAY_CLIENT_NAME,
            },
            celestia::CelestiaSecrets,
            eigen::EigenSecrets,
            DAClientConfig, AVAIL_CLIENT_CONFIG_NAME, CELESTIA_CLIENT_CONFIG_NAME,
            EIGEN_CLIENT_CONFIG_NAME, NO_DA_CLIENT_CONFIG_NAME, OBJECT_STORE_CLIENT_CONFIG_NAME,
        },
        secrets::DataAvailabilitySecrets,
        AvailConfig,
    },
    EigenConfig,
};

use crate::{envy_load, FromEnv};

pub fn da_client_config_from_env(prefix: &str) -> anyhow::Result<DAClientConfig> {
    let client_tag = env::var(format!("{}CLIENT", prefix))?;
    let config = match client_tag.as_str() {
        AVAIL_CLIENT_CONFIG_NAME => DAClientConfig::Avail(AvailConfig {
            bridge_api_url: env::var(format!("{}BRIDGE_API_URL", prefix))?,
            timeout_ms: env::var(format!("{}TIMEOUT_MS", prefix))?.parse()?,
            config: match env::var(format!("{}AVAIL_CLIENT_TYPE", prefix))?.as_str() {
                AVAIL_FULL_CLIENT_NAME => {
                    AvailClientConfig::FullClient(envy_load("da_avail_full_client", prefix)?)
                }
                AVAIL_GAS_RELAY_CLIENT_NAME => {
                    AvailClientConfig::GasRelay(envy_load("da_avail_gas_relay", prefix)?)
                }
                _ => anyhow::bail!("Unknown Avail DA client type"),
            },
        }),
        CELESTIA_CLIENT_CONFIG_NAME => {
            DAClientConfig::Celestia(envy_load("da_celestia_config", prefix)?)
        }
        EIGEN_CLIENT_CONFIG_NAME => DAClientConfig::Eigen(EigenConfig {
            disperser_rpc: env::var(format!("{}DISPERSER_RPC", prefix))?,
            settlement_layer_confirmation_depth: env::var(format!(
                "{}SETTLEMENT_LAYER_CONFIRMATION_DEPTH",
                prefix
            ))?
            .parse()?,
            eigenda_eth_rpc: match env::var(format!("{}EIGENDA_ETH_RPC", prefix)) {
                // Use a specific L1 RPC URL for the EigenDA client.
                Ok(url) => Some(SensitiveUrl::from_str(&url)?),
                // Err means that the environment variable is not set.
                // Use zkSync default L1 RPC for the EigenDA client.
                Err(_) => None,
            },
            eigenda_svc_manager_address: H160::from_str(&env::var(format!(
                "{}EIGENDA_SVC_MANAGER_ADDRESS",
                prefix
            ))?)?,
            wait_for_finalization: env::var(format!("{}WAIT_FOR_FINALIZATION", prefix))?.parse()?,
            authenticated: env::var(format!("{}AUTHENTICATED", prefix))?.parse()?,
            points_source: match env::var(format!("{}POINTS_SOURCE", prefix))?.as_str() {
                "Path" => zksync_config::configs::da_client::eigen::PointsSource::Path(env::var(
                    format!("{}POINTS_PATH", prefix),
                )?),
                "Url" => zksync_config::configs::da_client::eigen::PointsSource::Url((
                    env::var(format!("{}POINTS_LINK_G1", prefix))?,
                    env::var(format!("{}POINTS_LINK_G2", prefix))?,
                )),
                _ => anyhow::bail!("Unknown Eigen points type"),
            },
        }),
        OBJECT_STORE_CLIENT_CONFIG_NAME => {
            DAClientConfig::ObjectStore(envy_load("da_object_store", prefix)?)
        }
        NO_DA_CLIENT_CONFIG_NAME => DAClientConfig::NoDA,
        _ => anyhow::bail!("Unknown DA client name: {}", client_tag),
    };

    Ok(config)
}

impl FromEnv for DAClientConfig {
    fn from_env() -> anyhow::Result<Self> {
        da_client_config_from_env("DA_")
    }
}

pub fn da_client_secrets_from_env(prefix: &str) -> anyhow::Result<DataAvailabilitySecrets> {
    let client_tag = env::var(format!("{}CLIENT", prefix))?;
    let secrets = match client_tag.as_str() {
        AVAIL_CLIENT_CONFIG_NAME => {
            let seed_phrase: Option<zksync_basic_types::secrets::SeedPhrase> =
                env::var(format!("{}SECRETS_SEED_PHRASE", prefix))
                    .ok()
                    .map(Into::into);
            let gas_relay_api_key: Option<zksync_basic_types::secrets::APIKey> =
                env::var(format!("{}SECRETS_GAS_RELAY_API_KEY", prefix))
                    .ok()
                    .map(Into::into);
            if seed_phrase.is_none() && gas_relay_api_key.is_none() {
                anyhow::bail!("No secrets provided for Avail DA client");
            }
            DataAvailabilitySecrets::Avail(AvailSecrets {
                seed_phrase,
                gas_relay_api_key,
            })
        }
        CELESTIA_CLIENT_CONFIG_NAME => {
            let private_key = env::var(format!("{}SECRETS_PRIVATE_KEY", prefix))
                .context("Celestia private key not found")?
                .into();
            DataAvailabilitySecrets::Celestia(CelestiaSecrets { private_key })
        }
        EIGEN_CLIENT_CONFIG_NAME => {
            let private_key = env::var(format!("{}SECRETS_PRIVATE_KEY", prefix))
                .context("Eigen private key not found")?
                .into();
            DataAvailabilitySecrets::Eigen(EigenSecrets { private_key })
        }

        _ => anyhow::bail!("Unknown DA client name: {}", client_tag),
    };

    Ok(secrets)
}

impl FromEnv for DataAvailabilitySecrets {
    fn from_env() -> anyhow::Result<Self> {
        da_client_secrets_from_env("DA_")
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use zksync_basic_types::url::SensitiveUrl;
    use zksync_config::{
        configs::{
            da_client::{
                avail::{AvailClientConfig, AvailDefaultConfig},
                eigen::PointsSource,
                DAClientConfig::{self, ObjectStore},
            },
            object_store::ObjectStoreMode::GCS,
        },
        AvailConfig, CelestiaConfig, EigenConfig, ObjectStoreConfig,
    };

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_object_store_da_client_config(url: String, max_retries: u16) -> DAClientConfig {
        ObjectStore(ObjectStoreConfig {
            mode: GCS {
                bucket_base_url: url,
            },
            max_retries,
            local_mirror_path: None,
        })
    }

    #[test]
    fn from_env_object_store() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="ObjectStore"

            DA_BUCKET_BASE_URL="sometestpath"
            DA_MODE="GCS"
            DA_MAX_RETRIES="5"
        "#;
        lock.set_env(config);
        let actual = DAClientConfig::from_env().unwrap();
        assert_eq!(
            actual,
            expected_object_store_da_client_config("sometestpath".to_string(), 5)
        );
    }

    fn expected_avail_da_layer_config(
        api_node_url: &str,
        bridge_api_url: &str,
        app_id: u32,
        timeout_ms: usize,
    ) -> DAClientConfig {
        DAClientConfig::Avail(AvailConfig {
            bridge_api_url: bridge_api_url.to_string(),
            timeout_ms,
            config: AvailClientConfig::FullClient(AvailDefaultConfig {
                api_node_url: api_node_url.to_string(),
                app_id,
                finality_state: None,
            }),
        })
    }

    #[test]
    fn from_env_avail_client() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="Avail"
            DA_AVAIL_CLIENT_TYPE="FullClient"

            DA_BRIDGE_API_URL="localhost:54321"
            DA_TIMEOUT_MS="2000"

            DA_API_NODE_URL="localhost:12345"
            DA_APP_ID="1"
        "#;

        lock.set_env(config);

        let actual = DAClientConfig::from_env().unwrap();
        assert_eq!(
            actual,
            expected_avail_da_layer_config(
                "localhost:12345",
                "localhost:54321",
                "1".parse::<u32>().unwrap(),
                "2000".parse::<usize>().unwrap(),
            )
        );
    }

    #[test]
    fn from_env_avail_secrets() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="Avail"
            DA_SECRETS_SEED_PHRASE="bottom drive obey lake curtain smoke basket hold race lonely fit walk"
        "#;

        lock.set_env(config);

        let (actual_seed, actual_key) = match DataAvailabilitySecrets::from_env().unwrap() {
            DataAvailabilitySecrets::Avail(avail) => (avail.seed_phrase, avail.gas_relay_api_key),
            _ => {
                panic!("Avail config expected")
            }
        };
        assert_eq!(
            (actual_seed.unwrap(), actual_key),
            (
                "bottom drive obey lake curtain smoke basket hold race lonely fit walk".into(),
                None
            )
        );
    }

    fn expected_celestia_da_layer_config(
        api_node_url: &str,
        namespace: &str,
        chain_id: &str,
        timeout_ms: u64,
    ) -> DAClientConfig {
        DAClientConfig::Celestia(CelestiaConfig {
            api_node_url: api_node_url.to_string(),
            namespace: namespace.to_string(),
            chain_id: chain_id.to_string(),
            timeout_ms,
        })
    }

    #[test]
    fn from_env_celestia_client() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="Celestia"
            DA_API_NODE_URL="localhost:12345"
            DA_NAMESPACE="0x1234567890abcdef"
            DA_CHAIN_ID="mocha-4"
            DA_TIMEOUT_MS="7000"
        "#;
        lock.set_env(config);

        let actual = DAClientConfig::from_env().unwrap();
        assert_eq!(
            actual,
            expected_celestia_da_layer_config(
                "localhost:12345",
                "0x1234567890abcdef",
                "mocha-4",
                7000
            )
        );
    }

    #[test]
    fn from_env_eigen_client() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="Eigen"
            DA_DISPERSER_RPC="http://localhost:8080"
            DA_SETTLEMENT_LAYER_CONFIRMATION_DEPTH=0
            DA_EIGENDA_ETH_RPC="http://localhost:8545"
            DA_EIGENDA_SVC_MANAGER_ADDRESS="0x0000000000000000000000000000000000000123"
            DA_WAIT_FOR_FINALIZATION=true
            DA_AUTHENTICATED=false
            DA_POINTS_SOURCE="Path"
            DA_POINTS_PATH="resources"
        "#;
        lock.set_env(config);

        let actual = DAClientConfig::from_env().unwrap();
        assert_eq!(
            actual,
            DAClientConfig::Eigen(EigenConfig {
                disperser_rpc: "http://localhost:8080".to_string(),
                settlement_layer_confirmation_depth: 0,
                eigenda_eth_rpc: Some(SensitiveUrl::from_str("http://localhost:8545").unwrap()),
                eigenda_svc_manager_address: "0x0000000000000000000000000000000000000123"
                    .parse()
                    .unwrap(),
                wait_for_finalization: true,
                authenticated: false,
                points_source: PointsSource::Path("resources".to_string()),
            })
        );
    }

    #[test]
    fn from_env_celestia_secrets() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="Celestia"
            DA_SECRETS_PRIVATE_KEY="f55baf7c0e4e33b1d78fbf52f069c426bc36cff1aceb9bc8f45d14c07f034d73"
        "#;

        lock.set_env(config);

        let DataAvailabilitySecrets::Celestia(actual) =
            DataAvailabilitySecrets::from_env().unwrap()
        else {
            panic!("expected Celestia config")
        };
        assert_eq!(
            actual.private_key,
            "f55baf7c0e4e33b1d78fbf52f069c426bc36cff1aceb9bc8f45d14c07f034d73".into()
        );
    }
}
