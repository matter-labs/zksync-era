use clap::Subcommand;
use xshell::Shell;

use self::args::{new_migration::DatabaseNewMigrationArgs, DatabaseCommonArgs};
use crate::commands::dev::messages::{
    MSG_DATABASE_CHECK_SQLX_DATA_ABOUT, MSG_DATABASE_DROP_ABOUT, MSG_DATABASE_MIGRATE_ABOUT,
    MSG_DATABASE_NEW_MIGRATION_ABOUT, MSG_DATABASE_PREPARE_ABOUT, MSG_DATABASE_RESET_ABOUT,
    MSG_DATABASE_SETUP_ABOUT,
};

pub mod args;
mod check_sqlx_data;
mod drop;
mod migrate;
mod new_migration;
mod prepare;
pub mod reset;
mod setup;

#[derive(Subcommand, Debug)]
pub enum DatabaseCommands {
    #[clap(about = MSG_DATABASE_CHECK_SQLX_DATA_ABOUT)]
    CheckSqlxData(DatabaseCommonArgs),
    #[clap(about = MSG_DATABASE_DROP_ABOUT)]
    Drop(DatabaseCommonArgs),
    #[clap(about = MSG_DATABASE_MIGRATE_ABOUT)]
    Migrate(DatabaseCommonArgs),
    #[clap(about = MSG_DATABASE_NEW_MIGRATION_ABOUT)]
    NewMigration(DatabaseNewMigrationArgs),
    #[clap(about = MSG_DATABASE_PREPARE_ABOUT)]
    Prepare(DatabaseCommonArgs),
    #[clap(about = MSG_DATABASE_RESET_ABOUT)]
    Reset(DatabaseCommonArgs),
    #[clap(about = MSG_DATABASE_SETUP_ABOUT)]
    Setup(DatabaseCommonArgs),
}

pub async fn run(shell: &Shell, args: DatabaseCommands) -> anyhow::Result<()> {
    match args {
        DatabaseCommands::CheckSqlxData(args) => check_sqlx_data::run(shell, args).await,
        DatabaseCommands::Drop(args) => drop::run(shell, args).await,
        DatabaseCommands::Migrate(args) => migrate::run(shell, args).await,
        DatabaseCommands::NewMigration(args) => new_migration::run(shell, args).await,
        DatabaseCommands::Prepare(args) => prepare::run(shell, args).await,
        DatabaseCommands::Reset(args) => reset::run(shell, args).await,
        DatabaseCommands::Setup(args) => setup::run(shell, args).await,
    }
}
