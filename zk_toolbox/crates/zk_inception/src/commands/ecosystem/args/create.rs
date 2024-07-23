use std::path::PathBuf;

use clap::Parser;
use common::{Prompt, PromptConfirm, PromptSelect};
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use strum::{EnumIter, IntoEnumIterator};
use types::{L1Network, WalletCreation};

use crate::{
    commands::chain::{args::create::ChainCreateArgs, ChainCreateArgsFinal},
    messages::{
        MSG_ECOSYSTEM_NAME_PROMPT, MSG_L1_NETWORK_HELP, MSG_L1_NETWORK_PROMPT,
        MSG_LINK_TO_CODE_HELP, MSG_LINK_TO_CODE_PROMPT, MSG_LINK_TO_CODE_SELECTION_CLONE,
        MSG_LINK_TO_CODE_SELECTION_PATH, MSG_REPOSITORY_ORIGIN_PROMPT, MSG_START_CONTAINERS_HELP,
        MSG_START_CONTAINERS_PROMPT,
    },
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct EcosystemCreateArgs {
    #[arg(long)]
    pub ecosystem_name: Option<String>,
    #[clap(long, help = MSG_L1_NETWORK_HELP, value_enum)]
    pub l1_network: Option<L1Network>,
    #[clap(long, help = MSG_LINK_TO_CODE_HELP)]
    pub link_to_code: Option<String>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub chain: ChainCreateArgs,
    #[clap(
        long, help = MSG_START_CONTAINERS_HELP, default_missing_value = "true", num_args = 0..=1
    )]
    pub start_containers: Option<bool>,
}

impl EcosystemCreateArgs {
    pub fn fill_values_with_prompt(mut self) -> EcosystemCreateArgsFinal {
        let mut ecosystem_name = self
            .ecosystem_name
            .unwrap_or_else(|| Prompt::new(MSG_ECOSYSTEM_NAME_PROMPT).ask());
        ecosystem_name = slugify!(&ecosystem_name, separator = "_");

        let link_to_code = self.link_to_code.unwrap_or_else(|| {
            let link_to_code_selection =
                PromptSelect::new(MSG_REPOSITORY_ORIGIN_PROMPT, LinkToCodeSelection::iter()).ask();
            match link_to_code_selection {
                LinkToCodeSelection::Clone => "".to_string(),
                LinkToCodeSelection::Path => Prompt::new(MSG_LINK_TO_CODE_PROMPT).ask(),
            }
        });

        let l1_network = PromptSelect::new(MSG_L1_NETWORK_PROMPT, L1Network::iter()).ask();

        // Make the only chain as a default one
        self.chain.set_as_default = Some(true);

        let chain = self.chain.fill_values_with_prompt(0, &l1_network);

        let start_containers = self.start_containers.unwrap_or_else(|| {
            PromptConfirm::new(MSG_START_CONTAINERS_PROMPT)
                .default(true)
                .ask()
        });

        EcosystemCreateArgsFinal {
            ecosystem_name,
            l1_network,
            link_to_code,
            wallet_creation: chain.wallet_creation,
            wallet_path: chain.wallet_path.clone(),
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
    pub chain_args: ChainCreateArgsFinal,
    pub start_containers: bool,
}

impl EcosystemCreateArgsFinal {
    pub fn chain_config(&self) -> ChainCreateArgsFinal {
        self.chain_args.clone()
    }
}

#[derive(Debug, Clone, EnumIter, PartialEq, Eq)]
enum LinkToCodeSelection {
    Clone,
    Path,
}

impl std::fmt::Display for LinkToCodeSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkToCodeSelection::Clone => write!(f, "{MSG_LINK_TO_CODE_SELECTION_CLONE}"),
            LinkToCodeSelection::Path => write!(f, "{MSG_LINK_TO_CODE_SELECTION_PATH}"),
        }
    }
}
