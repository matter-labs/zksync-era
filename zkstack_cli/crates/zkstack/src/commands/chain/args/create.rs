use std::{path::PathBuf, str::FromStr};

use anyhow::{bail, Context};
use clap::{Parser, ValueEnum, ValueHint};
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use strum::{Display, EnumIter, IntoEnumIterator};
use zkstack_cli_common::{Prompt, PromptConfirm, PromptSelect};
use zkstack_cli_config::forge_interface::deploy_ecosystem::output::Erc20Token;
use zkstack_cli_types::{BaseToken, L1BatchCommitmentMode, L1Network, ProverMode, WalletCreation};
use zksync_basic_types::H160;

use crate::{
    defaults::L2_CHAIN_ID,
    messages::{
        MSG_BASE_TOKEN_ADDRESS_HELP, MSG_BASE_TOKEN_ADDRESS_PROMPT,
        MSG_BASE_TOKEN_ADDRESS_VALIDATOR_ERR, MSG_BASE_TOKEN_PRICE_DENOMINATOR_HELP,
        MSG_BASE_TOKEN_PRICE_DENOMINATOR_PROMPT, MSG_BASE_TOKEN_PRICE_NOMINATOR_HELP,
        MSG_BASE_TOKEN_PRICE_NOMINATOR_PROMPT, MSG_BASE_TOKEN_SELECTION_PROMPT, MSG_CHAIN_ID_HELP,
        MSG_CHAIN_ID_PROMPT, MSG_CHAIN_ID_VALIDATOR_ERR, MSG_CHAIN_NAME_PROMPT,
        MSG_EVM_EMULATOR_HELP, MSG_EVM_EMULATOR_PROMPT,
        MSG_L1_BATCH_COMMIT_DATA_GENERATOR_MODE_PROMPT, MSG_L1_COMMIT_DATA_GENERATOR_MODE_HELP,
        MSG_NUMBER_VALIDATOR_GREATHER_THAN_ZERO_ERR, MSG_NUMBER_VALIDATOR_NOT_ZERO_ERR,
        MSG_PROVER_MODE_HELP, MSG_PROVER_VERSION_PROMPT, MSG_SET_AS_DEFAULT_HELP,
        MSG_SET_AS_DEFAULT_PROMPT, MSG_WALLET_CREATION_HELP, MSG_WALLET_CREATION_PROMPT,
        MSG_WALLET_CREATION_VALIDATOR_ERR, MSG_WALLET_PATH_HELP, MSG_WALLET_PATH_INVALID_ERR,
        MSG_WALLET_PATH_PROMPT,
    },
};

// We need to duplicate it for using enum inside the arguments
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, Display, ValueEnum)]
enum L1BatchCommitmentModeInternal {
    Rollup,
    Validium,
}

impl From<L1BatchCommitmentModeInternal> for L1BatchCommitmentMode {
    fn from(val: L1BatchCommitmentModeInternal) -> Self {
        match val {
            L1BatchCommitmentModeInternal::Rollup => L1BatchCommitmentMode::Rollup,
            L1BatchCommitmentModeInternal::Validium => L1BatchCommitmentMode::Validium,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct ChainCreateArgs {
    #[arg(long)]
    chain_name: Option<String>,
    #[clap(long, help = MSG_CHAIN_ID_HELP)]
    chain_id: Option<ChainId>,
    #[clap(long, help = MSG_PROVER_MODE_HELP, value_enum)]
    prover_mode: Option<ProverMode>,
    #[clap(long, help = MSG_WALLET_CREATION_HELP, value_enum)]
    wallet_creation: Option<WalletCreation>,
    #[clap(long, help = MSG_WALLET_PATH_HELP, value_hint = ValueHint::FilePath)]
    wallet_path: Option<PathBuf>,
    #[clap(long, help = MSG_L1_COMMIT_DATA_GENERATOR_MODE_HELP)]
    l1_batch_commit_data_generator_mode: Option<L1BatchCommitmentModeInternal>,
    #[clap(long, help = MSG_BASE_TOKEN_ADDRESS_HELP)]
    base_token_address: Option<String>,
    #[clap(long, help = MSG_BASE_TOKEN_PRICE_NOMINATOR_HELP)]
    base_token_price_nominator: Option<u64>,
    #[clap(long, help = MSG_BASE_TOKEN_PRICE_DENOMINATOR_HELP)]
    base_token_price_denominator: Option<u64>,
    #[clap(long, help = MSG_SET_AS_DEFAULT_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub(crate) set_as_default: Option<bool>,
    #[clap(long, default_value = "false")]
    pub(crate) legacy_bridge: bool,
    #[arg(long, help = MSG_EVM_EMULATOR_HELP, default_missing_value = "true", num_args = 0..=1)]
    evm_emulator: Option<bool>,
    #[clap(long, help = "Whether to update git submodules of repo")]
    pub update_submodules: Option<bool>,
    #[clap(long, help = "Use tight ports allocation (no offset between chains)")]
    pub tight_ports: bool,
}

impl ChainCreateArgs {
    pub fn fill_values_with_prompt(
        self,
        number_of_chains: u32,
        l1_network: &L1Network,
        possible_erc20: Vec<Erc20Token>,
        link_to_code: String,
    ) -> anyhow::Result<ChainCreateArgsFinal> {
        let mut chain_name = self
            .chain_name
            .unwrap_or_else(|| Prompt::new(MSG_CHAIN_NAME_PROMPT).ask());
        chain_name = slugify!(&chain_name, separator = "_");

        let chain_id = self
            .chain_id
            .map(|v| match v {
                ChainId::Sequential => L2_CHAIN_ID + number_of_chains,
                ChainId::Id(v) => v,
            })
            .unwrap_or_else(|| {
                Prompt::new(MSG_CHAIN_ID_PROMPT)
                    .default(&(L2_CHAIN_ID + number_of_chains).to_string())
                    .ask()
            });

        let wallet_creation = if let Some(wallet) = self.wallet_creation {
            if wallet == WalletCreation::Localhost && *l1_network != L1Network::Localhost {
                bail!(MSG_WALLET_CREATION_VALIDATOR_ERR);
            } else {
                wallet
            }
        } else {
            PromptSelect::new(
                MSG_WALLET_CREATION_PROMPT,
                WalletCreation::iter().filter(|wallet| {
                    // Disable localhost wallets for external networks
                    if *l1_network == L1Network::Localhost {
                        true
                    } else {
                        *wallet != WalletCreation::Localhost
                    }
                }),
            )
            .ask()
        };

        let prover_version = self.prover_mode.unwrap_or_else(|| {
            PromptSelect::new(MSG_PROVER_VERSION_PROMPT, ProverMode::iter()).ask()
        });

        let l1_batch_commit_data_generator_mode =
            self.l1_batch_commit_data_generator_mode.unwrap_or_else(|| {
                PromptSelect::new(
                    MSG_L1_BATCH_COMMIT_DATA_GENERATOR_MODE_PROMPT,
                    L1BatchCommitmentModeInternal::iter(),
                )
                .ask()
            });

        let wallet_path: Option<PathBuf> = if wallet_creation == WalletCreation::InFile {
            Some(self.wallet_path.unwrap_or_else(|| {
                Prompt::new(MSG_WALLET_PATH_PROMPT)
                    .validate_with(|val: &String| {
                        PathBuf::from_str(val)
                            .map(|_| ())
                            .map_err(|_| MSG_WALLET_PATH_INVALID_ERR.to_string())
                    })
                    .ask()
            }))
        } else {
            None
        };

        let number_validator = |val: &String| -> Result<(), String> {
            let Ok(val) = val.parse::<u64>() else {
                return Err(MSG_NUMBER_VALIDATOR_NOT_ZERO_ERR.to_string());
            };
            if val == 0 {
                return Err(MSG_NUMBER_VALIDATOR_GREATHER_THAN_ZERO_ERR.to_string());
            }
            Ok(())
        };

        let base_token = if self.base_token_address.is_none()
            && self.base_token_price_denominator.is_none()
            && self.base_token_price_nominator.is_none()
        {
            let mut token_selection: Vec<_> =
                BaseTokenSelection::iter().map(|a| a.to_string()).collect();

            let erc20_tokens = &mut (possible_erc20
                .iter()
                .map(|t| format!("{:?}", t.address))
                .collect());
            token_selection.append(erc20_tokens);
            let base_token_selection =
                PromptSelect::new(MSG_BASE_TOKEN_SELECTION_PROMPT, token_selection).ask();
            match base_token_selection.as_str() {
                "Eth" => BaseToken::eth(),
                other => {
                    let address = if other == "Custom" {
                        Prompt::new(MSG_BASE_TOKEN_ADDRESS_PROMPT).ask()
                    } else {
                        H160::from_str(other)?
                    };
                    let nominator = Prompt::new(MSG_BASE_TOKEN_PRICE_NOMINATOR_PROMPT)
                        .validate_with(number_validator)
                        .ask();
                    let denominator = Prompt::new(MSG_BASE_TOKEN_PRICE_DENOMINATOR_PROMPT)
                        .validate_with(number_validator)
                        .ask();
                    BaseToken {
                        address,
                        nominator,
                        denominator,
                    }
                }
            }
        } else {
            let address = if let Some(address) = self.base_token_address {
                H160::from_str(&address).context(MSG_BASE_TOKEN_ADDRESS_VALIDATOR_ERR)?
            } else {
                Prompt::new(MSG_BASE_TOKEN_ADDRESS_PROMPT).ask()
            };

            let nominator = self.base_token_price_nominator.unwrap_or_else(|| {
                Prompt::new(MSG_BASE_TOKEN_PRICE_NOMINATOR_PROMPT)
                    .validate_with(number_validator)
                    .ask()
            });
            let denominator = self.base_token_price_denominator.unwrap_or_else(|| {
                Prompt::new(MSG_BASE_TOKEN_PRICE_DENOMINATOR_PROMPT)
                    .validate_with(number_validator)
                    .ask()
            });

            BaseToken {
                address,
                nominator,
                denominator,
            }
        };

        let evm_emulator = self.evm_emulator.unwrap_or_else(|| {
            PromptConfirm::new(MSG_EVM_EMULATOR_PROMPT)
                .default(false)
                .ask()
        });

        let set_as_default = self.set_as_default.unwrap_or_else(|| {
            PromptConfirm::new(MSG_SET_AS_DEFAULT_PROMPT)
                .default(true)
                .ask()
        });

        Ok(ChainCreateArgsFinal {
            chain_name,
            chain_id,
            prover_version,
            wallet_creation,
            l1_batch_commit_data_generator_mode: l1_batch_commit_data_generator_mode.into(),
            wallet_path,
            base_token,
            set_as_default,
            legacy_bridge: self.legacy_bridge,
            evm_emulator,
            link_to_code,
            update_submodules: self.update_submodules,
            tight_ports: self.tight_ports,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChainCreateArgsFinal {
    pub chain_name: String,
    pub chain_id: u32,
    pub prover_version: ProverMode,
    pub wallet_creation: WalletCreation,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub wallet_path: Option<PathBuf>,
    pub base_token: BaseToken,
    pub set_as_default: bool,
    pub legacy_bridge: bool,
    pub evm_emulator: bool,
    pub link_to_code: String,
    pub update_submodules: Option<bool>,
    pub tight_ports: bool,
}

#[derive(Debug, Clone, EnumIter, Display, PartialEq, Eq)]
enum BaseTokenSelection {
    Eth,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum ChainId {
    Sequential,
    Id(u32),
}

impl FromStr for ChainId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        (s == "sequential")
            .then_some(ChainId::Sequential)
            .or_else(|| s.parse::<u32>().ok().map(ChainId::Id))
            .ok_or_else(|| MSG_CHAIN_ID_VALIDATOR_ERR.to_string())
    }
}
