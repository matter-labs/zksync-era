use clap::{Parser, ValueEnum};
use strum::{Display, EnumIter, IntoEnumIterator};
use zkstack_cli_common::{Prompt, PromptSelect};

use crate::commands::dev::messages::{
    MSG_DATABASE_NEW_MIGRATION_DATABASE_HELP, MSG_DATABASE_NEW_MIGRATION_DB_PROMPT,
    MSG_DATABASE_NEW_MIGRATION_NAME_HELP, MSG_DATABASE_NEW_MIGRATION_NAME_PROMPT,
};

#[derive(Debug, Parser)]
pub struct DatabaseNewMigrationArgs {
    #[clap(long, help = MSG_DATABASE_NEW_MIGRATION_DATABASE_HELP)]
    pub database: Option<SelectedDatabase>,
    #[clap(long, help = MSG_DATABASE_NEW_MIGRATION_NAME_HELP)]
    pub name: Option<String>,
}

impl DatabaseNewMigrationArgs {
    pub fn fill_values_with_prompt(self) -> DatabaseNewMigrationArgsFinal {
        let selected_database = self.database.unwrap_or_else(|| {
            PromptSelect::new(
                MSG_DATABASE_NEW_MIGRATION_DB_PROMPT,
                SelectedDatabase::iter(),
            )
            .ask()
        });
        let name = self
            .name
            .unwrap_or_else(|| Prompt::new(MSG_DATABASE_NEW_MIGRATION_NAME_PROMPT).ask());

        DatabaseNewMigrationArgsFinal {
            selected_database,
            name,
        }
    }
}

#[derive(Debug)]
pub struct DatabaseNewMigrationArgsFinal {
    pub selected_database: SelectedDatabase,
    pub name: String,
}

#[derive(Debug, Clone, ValueEnum, EnumIter, PartialEq, Eq, Display)]
pub enum SelectedDatabase {
    Prover,
    Core,
}
