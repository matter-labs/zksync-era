use std::{path::PathBuf, str::FromStr};

use clap::{Parser, ValueEnum};
use common::{Prompt, PromptConfirm, PromptSelect};
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use strum::{Display, EnumIter, IntoEnumIterator};
use types::{BaseToken, L1BatchCommitmentMode, L1Network, ProverMode, WalletCreation};

use crate::{
    defaults::L2_CHAIN_ID,
    messages::{
        MSG_BASE_TOKEN_ADDRESS_HELP, MSG_BASE_TOKEN_ADDRESS_PROMPT,
        MSG_BASE_TOKEN_PRICE_DENOMINATOR_HELP, MSG_BASE_TOKEN_PRICE_DENOMINATOR_PROMPT,
        MSG_BASE_TOKEN_PRICE_NOMINATOR_HELP, MSG_BASE_TOKEN_PRICE_NOMINATOR_PROMPT,
        MSG_BASE_TOKEN_SELECTION_PROMPT, MSG_CHAIN_ID_PROMPT, MSG_CHAIN_NAME_PROMPT,
        MSG_L1_BATCH_COMMIT_DATA_GENERATOR_MODE_PROMPT, MSG_L1_COMMIT_DATA_GENERATOR_MODE_HELP,
        MSG_NUMBER_VALIDATOR_GREATHER_THAN_ZERO_ERR, MSG_NUMBER_VALIDATOR_NOT_ZERO_ERR,
        MSG_PROVER_MODE_HELP, MSG_PROVER_VERSION_PROMPT, MSG_SET_AS_DEFAULT_HELP,
        MSG_SET_AS_DEFAULT_PROMPT, MSG_WALLET_CREATION_HELP, MSG_WALLET_CREATION_PROMPT,
        MSG_WALLET_PATH_HELP, MSG_WALLET_PATH_INVALID_ERR, MSG_WALLET_PATH_PROMPT,
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
    #[arg(value_parser = clap::value_parser ! (u32).range(1..))]
    chain_id: Option<u32>,
    #[clap(long, help = MSG_PROVER_MODE_HELP, value_enum)]
    prover_mode: Option<ProverMode>,
    #[clap(long, help = MSG_WALLET_CREATION_HELP, value_enum)]
    wallet_creation: Option<WalletCreation>,
    #[clap(long, help = MSG_WALLET_PATH_HELP)]
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
}

impl ChainCreateArgs {
    pub fn fill_values_with_prompt(
        self,
        number_of_chains: u32,
        l1_network: &L1Network,
    ) -> ChainCreateArgsFinal {
        let mut chain_name = self
            .chain_name
            .unwrap_or_else(|| Prompt::new(MSG_CHAIN_NAME_PROMPT).ask());
        chain_name = slugify!(&chain_name, separator = "_");

        let chain_id = self.chain_id.unwrap_or_else(|| {
            Prompt::new(MSG_CHAIN_ID_PROMPT)
                .default(&(L2_CHAIN_ID + number_of_chains).to_string())
                .ask()
        });

        let wallet_creation = PromptSelect::new(
            MSG_WALLET_CREATION_PROMPT,
            WalletCreation::iter().filter(|wallet| {
                // Disable localhost wallets for external networks
                if l1_network == &L1Network::Localhost {
                    true
                } else {
                    wallet != &WalletCreation::Localhost
                }
            }),
        )
        .ask();

        let prover_version = PromptSelect::new(MSG_PROVER_VERSION_PROMPT, ProverMode::iter()).ask();

        let l1_batch_commit_data_generator_mode =
            self.l1_batch_commit_data_generator_mode.unwrap_or_else(|| {
                PromptSelect::new(
                    MSG_L1_BATCH_COMMIT_DATA_GENERATOR_MODE_PROMPT,
                    L1BatchCommitmentModeInternal::iter(),
                )
                .ask()
            });

        let wallet_path: Option<PathBuf> = if self.wallet_creation == Some(WalletCreation::InFile) {
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

        let base_token_selection =
            PromptSelect::new(MSG_BASE_TOKEN_SELECTION_PROMPT, BaseTokenSelection::iter()).ask();
        let base_token = match base_token_selection {
            BaseTokenSelection::Eth => BaseToken::eth(),
            BaseTokenSelection::Custom => {
                let number_validator = |val: &String| -> Result<(), String> {
                    let Ok(val) = val.parse::<u64>() else {
                        return Err(MSG_NUMBER_VALIDATOR_NOT_ZERO_ERR.to_string());
                    };
                    if val == 0 {
                        return Err(MSG_NUMBER_VALIDATOR_GREATHER_THAN_ZERO_ERR.to_string());
                    }
                    Ok(())
                };
                let address = Prompt::new(MSG_BASE_TOKEN_ADDRESS_PROMPT).ask();
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
        };

        let set_as_default = self.set_as_default.unwrap_or_else(|| {
            PromptConfirm::new(MSG_SET_AS_DEFAULT_PROMPT)
                .default(true)
                .ask()
        });

        ChainCreateArgsFinal {
            chain_name,
            chain_id,
            prover_version,
            wallet_creation,
            l1_batch_commit_data_generator_mode: l1_batch_commit_data_generator_mode.into(),
            wallet_path,
            base_token,
            set_as_default,
        }
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
}

#[derive(Debug, Clone, EnumIter, Display, PartialEq, Eq)]
enum BaseTokenSelection {
    Eth,
    Custom,
}
