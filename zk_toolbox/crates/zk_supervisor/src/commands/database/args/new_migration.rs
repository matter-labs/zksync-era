use clap::{Parser, ValueEnum};
use common::{Prompt, PromptSelect};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};

#[derive(Debug, Parser)]
pub struct DatabaseNewMigrationArgs {
    /// Database to create new migration for
    #[clap(long)]
    pub database: Option<SelectedDatabase>,
    /// Migration name
    #[clap(long)]
    pub name: Option<String>,
}

impl DatabaseNewMigrationArgs {
    pub fn fill_values_with_prompt(self) -> DatabaseNewMigrationArgsFinal {
        let selected_database = self.database.unwrap_or_else(|| {
            PromptSelect::new(
                "What database do you want to create a new migration for?",
                SelectedDatabase::iter(),
            )
            .ask()
        });
        let name = self
            .name
            .unwrap_or_else(|| Prompt::new("How do you want to name the migration?").ask());

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
