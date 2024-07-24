use std::path::{Path, PathBuf};

use anyhow::bail;
use clap::Parser;
use common::{cmd::Cmd, logger, Prompt, PromptConfirm, PromptSelect};
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use types::{L1Network, WalletCreation};
use xshell::{cmd, Shell};

use crate::{
    commands::chain::{args::create::ChainCreateArgs, ChainCreateArgsFinal},
    messages::{
        msg_path_to_zksync_does_not_exist_err, MSG_CONFIRM_STILL_USE_FOLDER,
        MSG_ECOSYSTEM_NAME_PROMPT, MSG_L1_NETWORK_HELP, MSG_L1_NETWORK_PROMPT,
        MSG_LINK_TO_CODE_HELP, MSG_LINK_TO_CODE_PROMPT, MSG_LINK_TO_CODE_SELECTION_CLONE,
        MSG_LINK_TO_CODE_SELECTION_PATH, MSG_NOT_MAIN_REPO_OR_FORK_ERR,
        MSG_REPOSITORY_ORIGIN_PROMPT, MSG_START_CONTAINERS_HELP, MSG_START_CONTAINERS_PROMPT,
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
    #[clap(long, help = MSG_START_CONTAINERS_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub start_containers: Option<bool>,
}

impl EcosystemCreateArgs {
    pub fn fill_values_with_prompt(mut self, shell: &Shell) -> EcosystemCreateArgsFinal {
        let mut ecosystem_name = self
            .ecosystem_name
            .unwrap_or_else(|| Prompt::new(MSG_ECOSYSTEM_NAME_PROMPT).ask());
        ecosystem_name = slugify!(&ecosystem_name, separator = "_");

        let link_to_code = self.link_to_code.unwrap_or_else(|| {
            let link_to_code_selection =
                PromptSelect::new(MSG_REPOSITORY_ORIGIN_PROMPT, LinkToCodeSelection::iter()).ask();
            match link_to_code_selection {
                LinkToCodeSelection::Clone => "".to_string(),
                LinkToCodeSelection::Path => {
                    let mut path: String = Prompt::new(MSG_LINK_TO_CODE_PROMPT).ask();
                    if let Err(err) = check_link_to_code(shell, &path) {
                        logger::warn(err);
                        if !PromptConfirm::new(MSG_CONFIRM_STILL_USE_FOLDER).ask() {
                            path = pick_new_link_to_code(shell);
                        }
                    }
                    path
                }
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

fn check_link_to_code(shell: &Shell, path: &str) -> anyhow::Result<()> {
    let path = Path::new(path);
    if !path.exists() {
        bail!(msg_path_to_zksync_does_not_exist_err(
            path.to_str().unwrap()
        ));
    }

    let _guard = shell.push_dir(path);
    let out = String::from_utf8(
        Cmd::new(cmd!(shell, "git remote -v"))
            .run_with_output()?
            .stdout,
    )?;

    if !out.contains("matter-labs/zksync-era") {
        bail!(MSG_NOT_MAIN_REPO_OR_FORK_ERR);
    }

    Ok(())
}

fn pick_new_link_to_code(shell: &Shell) -> String {
    let link_to_code: String = Prompt::new(MSG_LINK_TO_CODE_PROMPT).ask();
    match check_link_to_code(shell, &link_to_code) {
        Ok(_) => link_to_code,
        Err(err) => {
            logger::warn(err);
            if !PromptConfirm::new(MSG_CONFIRM_STILL_USE_FOLDER).ask() {
                pick_new_link_to_code(shell)
            } else {
                link_to_code
            }
        }
    }
}
