use clap::Parser;
use common::{logger, Prompt, PromptConfirm, PromptSelect};
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use strum::IntoEnumIterator;
use xshell::Shell;

use crate::{
    messages::{
        MSG_CONFIRM_STILL_USE_FOLDER, MSG_LINK_TO_CODE_HELP, MSG_LINK_TO_CODE_PROMPT,
        MSG_PROVER_SUBSYSTEM_NAME_PROMPT, MSG_REPOSITORY_ORIGIN_PROMPT, MSG_START_CONTAINERS_HELP,
        MSG_START_CONTAINERS_PROMPT,
    },
    utils::args::{check_link_to_code, pick_new_link_to_code, LinkToCodeSelection},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct ProverCreateArgs {
    #[arg(long)]
    pub subsystem_name: Option<String>,
    #[clap(long, help = MSG_LINK_TO_CODE_HELP)]
    pub link_to_code: Option<String>,
    #[clap(
        long, help = MSG_START_CONTAINERS_HELP, default_missing_value = "true", num_args = 0..=1
    )]
    pub start_containers: Option<bool>,
}

impl ProverCreateArgs {
    pub fn fill_values_with_prompt(
        mut self,
        shell: &Shell,
    ) -> anyhow::Result<ProverCreateArgsFinal> {
        let mut subsystem_name = self
            .subsystem_name
            .unwrap_or_else(|| Prompt::new(MSG_PROVER_SUBSYSTEM_NAME_PROMPT).ask());
        subsystem_name = slugify!(&subsystem_name, separator = "_");

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
        let start_containers = self.start_containers.unwrap_or_else(|| {
            PromptConfirm::new(MSG_START_CONTAINERS_PROMPT)
                .default(true)
                .ask()
        });

        Ok(ProverCreateArgsFinal {
            subsystem_name,
            link_to_code,
            start_containers,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProverCreateArgsFinal {
    pub subsystem_name: String,
    pub link_to_code: String,
    pub start_containers: bool,
}
