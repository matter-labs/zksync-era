use std::path::PathBuf;

use clap::Parser;
use common::{slugify, Prompt, PromptConfirm, PromptSelect};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};
use url::Url;

use crate::{
    commands::chain::{args::create::ChainCreateArgs, ChainCreateArgsFinal},
    defaults::LOCAL_RPC_URL,
    types::L1Network,
    wallets::WalletCreation,
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct EcosystemCreateArgs {
    #[arg(long)]
    pub ecosystem_name: Option<String>,
    #[clap(long, help = "L1 Network", value_enum)]
    pub l1_network: Option<L1Network>,
    #[clap(long, help = "L1 RPC URL")]
    pub l1_rpc_url: Option<String>,
    #[clap(long, help = "Code link")]
    pub link_to_code: Option<String>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub chain: ChainCreateArgs,
    #[clap(long, help = "Start reth and postgres containers after creation", default_missing_value = "true", num_args = 0..=1)]
    pub start_containers: Option<bool>,
}

impl EcosystemCreateArgs {
    pub fn fill_values_with_prompt(mut self) -> EcosystemCreateArgsFinal {
        let mut ecosystem_name = self
            .ecosystem_name
            .unwrap_or_else(|| Prompt::new("How do you want to name the ecosystem?").ask());
        ecosystem_name = slugify(&ecosystem_name);

        let link_to_code = self.link_to_code.unwrap_or_else(|| {
            let link_to_code_selection = PromptSelect::new(
                "Select the origin of zksync-era repository",
                LinkToCodeSelection::iter(),
            )
            .ask();
            match link_to_code_selection {
                LinkToCodeSelection::Clone => "".to_string(),
                LinkToCodeSelection::Path => Prompt::new("Where's the code located?").ask(),
            }
        });

        let l1_network = PromptSelect::new("Select the L1 network", L1Network::iter()).ask();

        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            let mut prompt = Prompt::new("What is the RPC URL of the L1 network?");
            if l1_network == L1Network::Localhost {
                prompt = prompt.default(LOCAL_RPC_URL);
            }
            prompt
                .validate_with(|val: &String| -> Result<(), String> {
                    Url::parse(val)
                        .map(|_| ())
                        .map_err(|_| "Invalid RPC url".to_string())
                })
                .ask()
        });

        // Make the only chain as a default one
        self.chain.set_as_default = Some(true);

        let chain = self.chain.fill_values_with_prompt(0);

        let start_containers = self.start_containers.unwrap_or_else(|| {
            PromptConfirm::new("Do you want to start containers after creating the ecosystem?")
                .default(true)
                .ask()
        });

        EcosystemCreateArgsFinal {
            ecosystem_name,
            l1_network,
            link_to_code,
            wallet_creation: chain.wallet_creation,
            wallet_path: chain.wallet_path.clone(),
            l1_rpc_url,
            chain_args: chain,
            start_containers,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EcosystemCreateArgsFinal {
    pub ecosystem_name: String,
    pub l1_network: L1Network,
    pub link_to_code: String,
    pub wallet_creation: WalletCreation,
    pub wallet_path: Option<PathBuf>,
    pub l1_rpc_url: String,
    pub chain_args: ChainCreateArgsFinal,
    pub start_containers: bool,
}

impl EcosystemCreateArgsFinal {
    pub fn chain_config(&self) -> ChainCreateArgsFinal {
        self.chain_args.clone()
    }
}

#[derive(Debug, Clone, EnumIter, Display, PartialEq, Eq)]
enum LinkToCodeSelection {
    #[strum(serialize = "Clone for me")]
    Clone,
    #[strum(serialize = "I have the code already")]
    Path,
}
