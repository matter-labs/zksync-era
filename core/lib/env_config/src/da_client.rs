use std::env;

use zksync_config::configs::{
    da_client::{
        avail::{
            AvailClientConfig, AvailSecrets, AVAIL_FULL_CLIENT_NAME, AVAIL_GAS_RELAY_CLIENT_NAME,
        },
        DAClientConfig, AVAIL_CLIENT_CONFIG_NAME, OBJECT_STORE_CLIENT_CONFIG_NAME,
    },
    secrets::DataAvailabilitySecrets,
    AvailConfig,
};

use crate::{envy_load, FromEnv};

impl FromEnv for DAClientConfig {
    fn from_env() -> anyhow::Result<Self> {
        let client_tag = env::var("DA_CLIENT")?;
        let config = match client_tag.as_str() {
            AVAIL_CLIENT_CONFIG_NAME => Self::Avail(AvailConfig {
                bridge_api_url: env::var("DA_BRIDGE_API_URL").ok().unwrap(),
                timeout: env::var("DA_TIMEOUT")?.parse()?,

                config: match env::var("DA_AVAIL_CLIENT_TYPE")?.as_str() {
                    AVAIL_FULL_CLIENT_NAME => {
                        AvailClientConfig::FullClient(envy_load("da_avail_full_client", "DA_")?)
                    }
                    AVAIL_GAS_RELAY_CLIENT_NAME => {
                        AvailClientConfig::GasRelay(envy_load("da_avail_gas_relay", "DA_")?)
                    }
                    _ => anyhow::bail!("Unknown Avail DA client type"),
                },
            }),
            OBJECT_STORE_CLIENT_CONFIG_NAME => {
                Self::ObjectStore(envy_load("da_object_store", "DA_")?)
            }
            _ => anyhow::bail!("Unknown DA client name: {}", client_tag),
        };

        Ok(config)
    }
}

impl FromEnv for DataAvailabilitySecrets {
    fn from_env() -> anyhow::Result<Self> {
        let client_tag = std::env::var("DA_CLIENT")?;
        let secrets = match client_tag.as_str() {
            AVAIL_CLIENT_CONFIG_NAME => {
                let seed_phrase: Option<zksync_basic_types::seed_phrase::SeedPhrase> =
                    env::var("DA_SECRETS_SEED_PHRASE")
                        .ok()
                        .map(|s| s.parse().unwrap());
                let gas_relay_api_key: Option<zksync_basic_types::api_key::APIKey> =
                    env::var("DA_SECRETS_GAS_RELAY_API_KEY")
                        .ok()
                        .map(|s| s.parse().unwrap());
                if seed_phrase.is_none() && gas_relay_api_key.is_none() {
                    anyhow::bail!("No secrets provided for Avail DA client");
                }
                Self::Avail(AvailSecrets {
                    seed_phrase,
                    gas_relay_api_key,
                })
            }
            _ => anyhow::bail!("Unknown DA client name: {}", client_tag),
        };

        Ok(secrets)
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
        AvailConfig, ObjectStoreConfig,
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
        timeout: usize,
    ) -> DAClientConfig {
        DAClientConfig::Avail(AvailConfig {
            bridge_api_url: bridge_api_url.to_string(),
            timeout,
            config: AvailClientConfig::FullClient(AvailDefaultConfig {
                api_node_url: api_node_url.to_string(),
                app_id,
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
            DA_TIMEOUT="2"

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
                "2".parse::<usize>().unwrap(),
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
        };

        assert_eq!(
            (actual_seed.unwrap(), actual_key),
            (
                "bottom drive obey lake curtain smoke basket hold race lonely fit walk"
                    .parse()
                    .unwrap(),
                None
            )
        );
    }
}
