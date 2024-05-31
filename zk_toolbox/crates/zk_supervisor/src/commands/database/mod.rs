use clap::Subcommand;
use xshell::Shell;

use self::args::{new_migration::DatabaseNewMigrationArgs, DatabaseCommonArgs};

mod args;
mod check_sqlx_data;
mod drop;
mod migrate;
mod new_migration;
mod prepare;
mod reset;
mod setup;

#[derive(Subcommand, Debug)]
pub enum DatabaseCommands {
    /// Check sqlx-data.json is up to date. If no databases are selected, all databases will be checked.
    CheckSqlxData(DatabaseCommonArgs),
    /// Drop databases. If no databases are selected, all databases will be dropped.
    Drop(DatabaseCommonArgs),
    /// Migrate databases. If no databases are selected, all databases will be migrated.
    Migrate(DatabaseCommonArgs),
    /// Create new migration
    NewMigration(DatabaseNewMigrationArgs),
    /// Prepare sqlx-data.json. If no databases are selected, all databases will be prepared.
    Prepare(DatabaseCommonArgs),
    /// Reset databases. If no databases are selected, all databases will be reset.
    Reset(DatabaseCommonArgs),
    /// Setup databases. If no databases are selected, all databases will be setup.
    Setup(DatabaseCommonArgs),
}

pub async fn run(shell: &Shell, args: DatabaseCommands) -> anyhow::Result<()> {
    match args {
        DatabaseCommands::CheckSqlxData(args) => check_sqlx_data::run(shell, args),
        DatabaseCommands::Drop(args) => drop::run(shell, args).await,
        DatabaseCommands::Migrate(args) => migrate::run(shell, args),
        DatabaseCommands::NewMigration(args) => new_migration::run(shell, args),
        DatabaseCommands::Prepare(args) => prepare::run(shell, args),
        DatabaseCommands::Reset(args) => reset::run(shell, args).await,
        DatabaseCommands::Setup(args) => setup::run(shell, args),
    }
}
