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
    /// Check sqlx-data.json is up to date
    CheckSqlxData(DatabaseCommonArgs),
    /// Drop databases
    Drop(DatabaseCommonArgs),
    /// Migrate databases
    Migrate(DatabaseCommonArgs),
    /// Create new migration
    NewMigration(DatabaseNewMigrationArgs),
    /// Prepare sqlx-data.json
    Prepare(DatabaseCommonArgs),
    /// Reset databases
    Reset(DatabaseCommonArgs),
    /// Setup databases
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
