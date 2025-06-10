//! Legacy env var parsing logic. Will be removed after the transition to `smart-config` is complete for EN.

use std::{env, str::FromStr, time::Duration};

use anyhow::Context;
use serde::de::DeserializeOwned;
use zksync_config::{
    configs::{
        da_client::{
            avail::{AvailClientConfig, AvailSecrets},
            celestia::CelestiaSecrets,
            eigen::EigenSecrets,
        },
        DataAvailabilitySecrets,
    },
    AvailConfig, DAClientConfig, EigenConfig,
};
use zksync_types::{
    secrets::{APIKey, PrivateKey, SeedPhrase},
    url::SensitiveUrl,
};

fn envy_load<T: DeserializeOwned>(name: &str, prefix: &str) -> anyhow::Result<T> {
    envy::prefixed(prefix)
        .from_env()
        .map_err(|e| anyhow::anyhow!("Failed to load {} from env: {}", name, e))
}

const AVAIL_CLIENT_CONFIG_NAME: &str = "Avail";
const CELESTIA_CLIENT_CONFIG_NAME: &str = "Celestia";
const EIGEN_CLIENT_CONFIG_NAME: &str = "Eigen";
const OBJECT_STORE_CLIENT_CONFIG_NAME: &str = "ObjectStore";
const NO_DA_CLIENT_CONFIG_NAME: &str = "NoDA";
const AVAIL_GAS_RELAY_CLIENT_NAME: &str = "GasRelay";
const AVAIL_FULL_CLIENT_NAME: &str = "FullClient";

pub fn da_client_config_from_env(prefix: &str) -> anyhow::Result<DAClientConfig> {
    let client_tag = env::var(format!("{}CLIENT", prefix))?;
    let config = match client_tag.as_str() {
        AVAIL_CLIENT_CONFIG_NAME => DAClientConfig::Avail(AvailConfig {
            bridge_api_url: env::var(format!("{}BRIDGE_API_URL", prefix))?,
            timeout: Duration::from_millis(env::var(format!("{}TIMEOUT_MS", prefix))?.parse()?),
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
            eigenda_eth_rpc: match env::var(format!("{}EIGENDA_ETH_RPC", prefix)) {
                // Use a specific L1 RPC URL for the EigenDA client.
                Ok(url) => Some(SensitiveUrl::from_str(&url)?),
                // Err means that the environment variable is not set.
                // Use zkSync default L1 RPC for the EigenDA client.
                Err(_) => None,
            },
            authenticated: env::var(format!("{}AUTHENTICATED", prefix))?.parse()?,
            cert_verifier_router_addr: env::var(format!(
                "{}EIGENDA_CERT_VERIFIER_ROUTER_ADDR",
                prefix
            ))?,
            operator_state_retriever_addr: env::var(format!(
                "{}EIGENDA_OPERATOR_STATE_RETRIEVER_ADDR",
                prefix
            ))?,
            registry_coordinator_addr: env::var(format!(
                "{}EIGENDA_REGISTRY_COORDINATOR_ADDR",
                prefix
            ))?,
            blob_version: env::var(format!("{}BLOB_VERSION", prefix))?
                .parse()
                .context("EigenDA blob version not found")?,
            polynomial_form: match env::var(format!("{}POLYNOMIAL_FORM", prefix))?.as_str() {
                "Coeff" => zksync_config::configs::da_client::eigen::PolynomialForm::Coeff,
                "Eval" => zksync_config::configs::da_client::eigen::PolynomialForm::Eval,
                _ => anyhow::bail!("Unknown Eigen polynomial form"),
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

pub fn da_client_secrets_from_env(prefix: &str) -> anyhow::Result<DataAvailabilitySecrets> {
    let client_tag = env::var(format!("{}CLIENT", prefix))?;
    let secrets = match client_tag.as_str() {
        AVAIL_CLIENT_CONFIG_NAME => {
            let seed_phrase = env::var(format!("{}SECRETS_SEED_PHRASE", prefix))
                .ok()
                .map(|s| SeedPhrase(s.into()));
            let gas_relay_api_key = env::var(format!("{}SECRETS_GAS_RELAY_API_KEY", prefix))
                .ok()
                .map(|s| APIKey(s.into()));
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
                .context("Celestia private key not found")?;
            DataAvailabilitySecrets::Celestia(CelestiaSecrets {
                private_key: PrivateKey(private_key.into()),
            })
        }
        EIGEN_CLIENT_CONFIG_NAME => {
            let private_key = env::var(format!("{}SECRETS_PRIVATE_KEY", prefix))
                .context("Eigen private key not found")?;
            DataAvailabilitySecrets::Eigen(EigenSecrets {
                private_key: PrivateKey(private_key.into()),
            })
        }

        _ => anyhow::bail!("Unknown DA client name: {}", client_tag),
    };

    Ok(secrets)
}
