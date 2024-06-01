use clap::Parser;

use crate::{
    dals::SelectedDals,
    messages::{MSG_DATABASE_COMMON_CORE_HELP, MSG_DATABASE_COMMON_PROVER_HELP},
};

pub mod new_migration;

#[derive(Debug, Parser)]
pub struct DatabaseCommonArgs {
    #[clap(short, long, default_missing_value = "true", num_args = 0..=1, help = MSG_DATABASE_COMMON_PROVER_HELP)]
    pub prover: Option<bool>,
    #[clap(short, long, default_missing_value = "true", num_args = 0..=1, help = MSG_DATABASE_COMMON_CORE_HELP)]
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
