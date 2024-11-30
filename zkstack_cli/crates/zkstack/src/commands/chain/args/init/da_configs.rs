use clap::ValueEnum;
use common::{Prompt, PromptSelect};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, IntoEnumIterator};
use url::Url;
use zksync_config::{
    configs::da_client::avail::{AvailClientConfig, AvailDefaultConfig, AvailGasRelayConfig},
    AvailConfig,
};

use crate::{
    defaults::{AVAIL_BRIDGE_API_URL, AVAIL_RPC_URL},
    messages::{
        MSG_AVAIL_API_NODE_URL_PROMPT, MSG_AVAIL_API_TIMEOUT_MS, MSG_AVAIL_APP_ID_PROMPT,
        MSG_AVAIL_BRIDGE_API_URL_PROMPT, MSG_AVAIL_CLIENT_TYPE_PROMPT,
        MSG_AVAIL_GAS_RELAY_API_URL_PROMPT, MSG_AVAIL_GAS_RELAY_MAX_RETRIES_PROMPT,
        MSG_INVALID_URL_ERR, MSG_VALIDIUM_TYPE_PROMPT,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, Display, ValueEnum)]
pub enum ValidiumTypeInternal {
    NoDA,
    Avail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, Display, ValueEnum)]
pub enum AvailClientTypeInternal {
    FullClient,
    GasRelay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValidiumType {
    NoDA,
    Avail(AvailConfig),
}

impl ValidiumType {
    pub fn read() -> Self {
        match PromptSelect::new(MSG_VALIDIUM_TYPE_PROMPT, ValidiumTypeInternal::iter()).ask() {
            ValidiumTypeInternal::NoDA => ValidiumType::NoDA,
            ValidiumTypeInternal::Avail => {
                let client_config =
                    match PromptSelect::new(
                        MSG_AVAIL_CLIENT_TYPE_PROMPT,
                        AvailClientTypeInternal::iter(),
                    )
                    .ask()
                    {
                        AvailClientTypeInternal::FullClient => {
                            AvailClientConfig::FullClient(AvailDefaultConfig {
                                api_node_url: Prompt::new(MSG_AVAIL_API_NODE_URL_PROMPT)
                                    .default(AVAIL_RPC_URL.as_str())
                                    .validate_with(url_validator)
                                    .ask(),
                                app_id: Prompt::new(MSG_AVAIL_APP_ID_PROMPT)
                                    .validate_with(|input: &String| -> Result<(), String> {
                                        input.parse::<u32>().map(|_| ()).map_err(|_| {
                                            "Please enter a positive number".to_string()
                                        })
                                    })
                                    .ask(),
                            })
                        }
                        AvailClientTypeInternal::GasRelay => {
                            AvailClientConfig::GasRelay(AvailGasRelayConfig {
                                gas_relay_api_url: Prompt::new(MSG_AVAIL_GAS_RELAY_API_URL_PROMPT)
                                    .validate_with(url_validator)
                                    .ask(),
                                max_retries: Prompt::new(MSG_AVAIL_GAS_RELAY_MAX_RETRIES_PROMPT)
                                    .validate_with(|input: &String| -> Result<(), String> {
                                        input.parse::<usize>().map(|_| ()).map_err(|_| {
                                            "Please enter a positive number".to_string()
                                        })
                                    })
                                    .ask(),
                            })
                        }
                    };

                ValidiumType::Avail(AvailConfig {
                    bridge_api_url: Prompt::new(MSG_AVAIL_BRIDGE_API_URL_PROMPT)
                        .default(AVAIL_BRIDGE_API_URL.as_str())
                        .validate_with(url_validator)
                        .ask(),
                    timeout_ms: Prompt::new(MSG_AVAIL_API_TIMEOUT_MS)
                        .validate_with(|input: &String| -> Result<(), String> {
                            input
                                .parse::<usize>()
                                .map(|_| ())
                                .map_err(|_| "Please enter a positive number".to_string())
                        })
                        .ask(),
                    config: client_config,
                })
            }
        }
    }
}

fn url_validator(val: &String) -> Result<(), String> {
    Url::parse(val)
        .map(|_| ())
        .map_err(|_| MSG_INVALID_URL_ERR.to_string())
}
