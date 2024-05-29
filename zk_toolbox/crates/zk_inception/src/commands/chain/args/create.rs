use std::{path::PathBuf, str::FromStr};

use clap::Parser;
use common::{slugify, Prompt, PromptConfirm, PromptSelect};
use ethers::types::H160;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};

use crate::{
    defaults::L2_CHAIN_ID,
    types::{BaseToken, L1BatchCommitDataGeneratorMode, ProverMode},
    wallets::WalletCreation,
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct ChainCreateArgs {
    #[arg(long)]
    pub chain_name: Option<String>,
    #[arg(value_parser = clap::value_parser!(u32).range(1..))]
    pub chain_id: Option<u32>,
    #[clap(long, help = "Prover options", value_enum)]
    pub prover_mode: Option<ProverMode>,
    #[clap(long, help = "Wallet option", value_enum)]
    pub wallet_creation: Option<WalletCreation>,
    #[clap(long, help = "Wallet path")]
    pub wallet_path: Option<PathBuf>,
    #[clap(long, help = "Commit data generation mode")]
    pub l1_batch_commit_data_generator_mode: Option<L1BatchCommitDataGeneratorMode>,
    #[clap(long, help = "Base token address")]
    pub base_token_address: Option<String>,
    #[clap(long, help = "Base token nominator")]
    pub base_token_price_nominator: Option<u64>,
    #[clap(long, help = "Base token denominator")]
    pub base_token_price_denominator: Option<u64>,
    #[clap(long, help = "Set as default chain", default_missing_value = "true", num_args = 0..=1)]
    pub set_as_default: Option<bool>,
}

impl ChainCreateArgs {
    pub fn fill_values_with_prompt(self, number_of_chains: u32) -> ChainCreateArgsFinal {
        let mut chain_name = self
            .chain_name
            .unwrap_or_else(|| Prompt::new("How do you want to name the chain?").ask());
        chain_name = slugify(&chain_name);

        let chain_id = self.chain_id.unwrap_or_else(|| {
            Prompt::new("What's the chain id?")
                .default(&(L2_CHAIN_ID + number_of_chains).to_string())
                .ask()
        });

        let wallet_creation = PromptSelect::new(
            "Select how do you want to create the wallet",
            WalletCreation::iter(),
        )
        .ask();

        let prover_version =
            PromptSelect::new("Select the prover version", ProverMode::iter()).ask();

        let l1_batch_commit_data_generator_mode = PromptSelect::new(
            "Select the commit data generator mode",
            L1BatchCommitDataGeneratorMode::iter(),
        )
        .ask();

        let wallet_path: Option<PathBuf> = if self.wallet_creation == Some(WalletCreation::InFile) {
            Some(self.wallet_path.unwrap_or_else(|| {
                Prompt::new("What is the wallet path?")
                    .validate_with(|val: &String| {
                        PathBuf::from_str(val)
                            .map(|_| ())
                            .map_err(|_| "Invalid path".to_string())
                    })
                    .ask()
            }))
        } else {
            None
        };

        let base_token_selection =
            PromptSelect::new("Select the base token to use", BaseTokenSelection::iter()).ask();
        let base_token = match base_token_selection {
            BaseTokenSelection::Eth => BaseToken::eth(),
            BaseTokenSelection::Custom => {
                let number_validator = |val: &String| -> Result<(), String> {
                    let Ok(val) = val.parse::<u64>() else {
                        return Err("Numer is not zero".to_string());
                    };
                    if val == 0 {
                        return Err("Number should be greater than 0".to_string());
                    }
                    Ok(())
                };
                let address: H160 = Prompt::new("What is the base token address?").ask();
                let nominator = Prompt::new("What is the base token price nominator?")
                    .validate_with(number_validator)
                    .ask();
                let denominator = Prompt::new("What is the base token price denominator?")
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
            PromptConfirm::new("Set this chain as default?")
                .default(true)
                .ask()
        });

        ChainCreateArgsFinal {
            chain_name,
            chain_id,
            prover_version,
            wallet_creation,
            l1_batch_commit_data_generator_mode,
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
    pub l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
    pub wallet_path: Option<PathBuf>,
    pub base_token: BaseToken,
    pub set_as_default: bool,
}

#[derive(Debug, Clone, EnumIter, Display, PartialEq, Eq)]
enum BaseTokenSelection {
    Eth,
    Custom,
}
