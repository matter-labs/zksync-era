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
    pub fn parse(self) -> DatabaseCommonArgsFinal {
        if self.prover.is_none() && self.core.is_none() {
            return DatabaseCommonArgsFinal {
                selected_dals: SelectedDals {
                    prover: true,
                    core: true,
                },
            };
        }

        DatabaseCommonArgsFinal {
            selected_dals: SelectedDals {
                prover: self.prover.unwrap_or(false),
                core: self.core.unwrap_or(false),
            },
        }
    }
}

#[derive(Debug)]
pub struct DatabaseCommonArgsFinal {
    pub selected_dals: SelectedDals,
}
