use clap::Parser;

use crate::dals::SelectedDals;

pub mod new_migration;

#[derive(Debug, Parser)]
pub struct DatabaseCommonArgs {
    /// Prover
    #[clap(short, long, default_missing_value = "true", num_args = 0..=1)]
    pub prover: Option<bool>,
    /// Core
    #[clap(short, long, default_missing_value = "true", num_args = 0..=1)]
    pub core: Option<bool>,
}

impl DatabaseCommonArgs {
    pub fn fill_values_with_prompt(self, verb: &str) -> DatabaseCommonArgsFinal {
        let prover = self.prover.unwrap_or_else(|| {
            common::PromptConfirm::new(format!("Do you want to {verb} the prover database?")).ask()
        });

        let core = self.core.unwrap_or_else(|| {
            common::PromptConfirm::new(format!("Do you want to {verb} the core database?")).ask()
        });

        DatabaseCommonArgsFinal {
            selected_dals: SelectedDals { prover, core },
        }
    }
}

#[derive(Debug)]
pub struct DatabaseCommonArgsFinal {
    pub selected_dals: SelectedDals,
}
