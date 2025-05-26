use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, IntoEnumIterator};
use url::Url;
use zkstack_cli_common::{Prompt, PromptSelect};
use zkstack_cli_config::da::{
    AvailClientConfig, AvailConfig, AvailDefaultConfig, AvailGasRelayConfig, AvailSecrets,
};

use crate::{
    defaults::{AVAIL_BRIDGE_API_URL, AVAIL_RPC_URL},
    messages::{
        MSG_AVAIL_API_NODE_URL_PROMPT, MSG_AVAIL_API_TIMEOUT_MS, MSG_AVAIL_APP_ID_PROMPT,
        MSG_AVAIL_BRIDGE_API_URL_PROMPT, MSG_AVAIL_CLIENT_TYPE_PROMPT,
        MSG_AVAIL_FINALITY_STATE_PROMPT, MSG_AVAIL_GAS_RELAY_API_KEY_PROMPT,
        MSG_AVAIL_GAS_RELAY_API_URL_PROMPT, MSG_AVAIL_GAS_RELAY_MAX_RETRIES_PROMPT,
        MSG_AVAIL_SEED_PHRASE_PROMPT, MSG_INVALID_URL_ERR, MSG_VALIDIUM_TYPE_PROMPT,
    },
};

#[derive(Debug, Serialize, Deserialize, Parser, Clone)]
pub struct ValidiumTypeArgs {
    #[clap(long, help = "Type of the Validium network")]
    pub validium_type: Option<ValidiumTypeInternal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, Display, ValueEnum)]
pub enum ValidiumTypeInternal {
    NoDA,
    Avail,
    EigenDA,
    EigenDAM1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, Display, ValueEnum)]
pub enum AvailClientTypeInternal {
    FullClient,
    GasRelay,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidiumType {
    NoDA,
    Avail((AvailConfig, AvailSecrets)),
    EigenDA,
    EigenDAM1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, Display, ValueEnum)]
pub enum AvailFinalityState {
    InBlock,
    Finalized,
}

impl ValidiumType {
    pub fn read() -> Self {
        match PromptSelect::new(MSG_VALIDIUM_TYPE_PROMPT, ValidiumTypeInternal::iter()).ask() {
            ValidiumTypeInternal::EigenDA => ValidiumType::EigenDA, // EigenDA doesn't support configuration through CLI
            ValidiumTypeInternal::EigenDAM1 => ValidiumType::EigenDAM1, // EigenDA doesn't support configuration through CLI
            ValidiumTypeInternal::NoDA => ValidiumType::NoDA,
            ValidiumTypeInternal::Avail => {
                let avail_client_type = PromptSelect::new(
                    MSG_AVAIL_CLIENT_TYPE_PROMPT,
                    AvailClientTypeInternal::iter(),
                )
                .ask();

                let client_config =
                    match avail_client_type {
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
                                finality_state: Some(
                                    PromptSelect::new(
                                        MSG_AVAIL_FINALITY_STATE_PROMPT,
                                        AvailFinalityState::iter(),
                                    )
                                    .ask()
                                    .to_string(),
                                ),
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

                let avail_config = AvailConfig {
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
                };

                let avail_secrets = match avail_client_type {
                    AvailClientTypeInternal::FullClient => AvailSecrets {
                        seed_phrase: Some(Prompt::new(MSG_AVAIL_SEED_PHRASE_PROMPT).ask()),
                        gas_relay_api_key: None,
                    },
                    AvailClientTypeInternal::GasRelay => AvailSecrets {
                        seed_phrase: None,
                        gas_relay_api_key: Some(
                            Prompt::new(MSG_AVAIL_GAS_RELAY_API_KEY_PROMPT).ask(),
                        ),
                    },
                };

                ValidiumType::Avail((avail_config, avail_secrets))
            }
        }
    }
}

#[allow(clippy::ptr_arg)]
fn url_validator(val: &String) -> Result<(), String> {
    Url::parse(val)
        .map(|_| ())
        .map_err(|_| MSG_INVALID_URL_ERR.to_string())
}
