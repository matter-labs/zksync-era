use clap::Parser;

use crate::messages::{MSG_OBSERVABILITY_HELP, MSG_OBSERVABILITY_RUN_PROMPT};

#[derive(Debug, Parser)]
pub struct ContainersArgs {
    #[clap(long, short = 'o', help = MSG_OBSERVABILITY_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub observability: Option<bool>,
}

#[derive(Debug)]
pub struct ContainersArgsFinal {
    pub observability: bool,
}

impl ContainersArgs {
    pub fn fill_values_with_prompt(self) -> ContainersArgsFinal {
        let observability = self.observability.unwrap_or_else(|| {
            zkstack_cli_common::PromptConfirm::new(MSG_OBSERVABILITY_RUN_PROMPT)
                .default(true)
                .ask()
        });

        ContainersArgsFinal { observability }
    }
}
