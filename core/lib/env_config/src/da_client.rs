use std::env;

use zksync_config::configs::{
    da_client::{
        avail::{
            AvailClientConfig, AvailSecrets, AVAIL_FULL_CLIENT_NAME, AVAIL_GAS_RELAY_CLIENT_NAME,
        },
        celestia::CelestiaSecrets,
        eigen::EigenSecrets,
        DAClientConfig, AVAIL_CLIENT_CONFIG_NAME, CELESTIA_CLIENT_CONFIG_NAME,
        EIGEN_CLIENT_CONFIG_NAME, NO_DA_CLIENT_CONFIG_NAME, OBJECT_STORE_CLIENT_CONFIG_NAME,
    },
    secrets::DataAvailabilitySecrets,
    AvailConfig,
};

use crate::{envy_load, FromEnv};

pub fn da_client_config_from_env(prefix: &str) -> anyhow::Result<DAClientConfig> {
    let client_tag = env::var(format!("{}CLIENT", prefix))?;
    let config = match client_tag.as_str() {
        AVAIL_CLIENT_CONFIG_NAME => DAClientConfig::Avail(AvailConfig {
            bridge_api_url: env::var(format!("{}BRIDGE_API_URL", prefix)).ok().unwrap(),
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
        EIGEN_CLIENT_CONFIG_NAME => DAClientConfig::Eigen(envy_load("da_eigen_config", "DA_")?),
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
                .map_err(|e| anyhow::format_err!("Celestia private key not found: {}", e))?
                .into();
            DataAvailabilitySecrets::Celestia(CelestiaSecrets { private_key })
        }
        EIGEN_CLIENT_CONFIG_NAME => {
            let private_key = env::var(format!("{}SECRETS_PRIVATE_KEY", prefix))
                .map_err(|e| anyhow::format_err!("Eigen private key not found: {}", e))?
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
    use zksync_config::{
        configs::{
            da_client::{
                avail::{AvailClientConfig, AvailDefaultConfig},
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
            DA_RPC_NODE_URL="localhost:12345"
            DA_INCLUSION_POLLING_INTERVAL_MS="1000"
        "#;
        lock.set_env(config);

        let actual = DAClientConfig::from_env().unwrap();
        assert_eq!(
            actual,
            DAClientConfig::Eigen(EigenConfig {
                rpc_node_url: "localhost:12345".to_string(),
                inclusion_polling_interval_ms: 1000,
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
