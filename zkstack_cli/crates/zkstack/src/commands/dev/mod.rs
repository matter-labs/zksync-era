use clap::Subcommand;
use commands::status::args::StatusArgs;
use messages::MSG_STATUS_ABOUT;
use xshell::Shell;

use self::commands::{
    clean::CleanCommands, config_writer::ConfigWriterArgs, contracts::ContractsArgs,
    database::DatabaseCommands, fmt::FmtArgs, lint::LintArgs, prover::ProverCommands,
    send_transactions::args::SendTransactionsArgs, snapshot::SnapshotCommands, test::TestCommands,
};
use crate::commands::dev::messages::{
    MSG_CONFIG_WRITER_ABOUT, MSG_CONTRACTS_ABOUT, MSG_GENERATE_GENESIS_ABOUT,
    MSG_PROVER_VERSION_ABOUT, MSG_SEND_TXNS_ABOUT, MSG_SUBCOMMAND_CLEAN,
    MSG_SUBCOMMAND_DATABASE_ABOUT, MSG_SUBCOMMAND_FMT_ABOUT, MSG_SUBCOMMAND_LINT_ABOUT,
    MSG_SUBCOMMAND_SNAPSHOTS_CREATOR_ABOUT, MSG_SUBCOMMAND_TESTS_ABOUT,
};

mod commands;
mod consts;
mod dals;
mod defaults;
mod messages;

#[derive(Subcommand, Debug)]
pub enum DevCommands {
    #[command(subcommand, about = MSG_SUBCOMMAND_DATABASE_ABOUT, alias = "db")]
    Database(DatabaseCommands),
    #[command(subcommand, about = MSG_SUBCOMMAND_TESTS_ABOUT, alias = "t")]
    Test(TestCommands),
    #[command(subcommand, about = MSG_SUBCOMMAND_CLEAN)]
    Clean(CleanCommands),
    #[command(subcommand, about = MSG_SUBCOMMAND_SNAPSHOTS_CREATOR_ABOUT)]
    Snapshot(SnapshotCommands),
    #[command(about = MSG_SUBCOMMAND_LINT_ABOUT, alias = "l")]
    Lint(LintArgs),
    #[command(about = MSG_SUBCOMMAND_FMT_ABOUT)]
    Fmt(FmtArgs),
    #[command(subcommand, about = MSG_PROVER_VERSION_ABOUT)]
    Prover(ProverCommands),
    #[command(about = MSG_CONTRACTS_ABOUT)]
    Contracts(ContractsArgs),
    #[command(about = MSG_CONFIG_WRITER_ABOUT, alias = "o")]
    ConfigWriter(ConfigWriterArgs),
    #[command(about = MSG_SEND_TXNS_ABOUT)]
    SendTransactions(SendTransactionsArgs),
    #[command(about = MSG_STATUS_ABOUT)]
    Status(StatusArgs),
    #[command(about = MSG_GENERATE_GENESIS_ABOUT, alias = "genesis")]
    GenerateGenesis,
}

pub async fn run(shell: &Shell, args: DevCommands) -> anyhow::Result<()> {
    match args {
        DevCommands::Database(command) => commands::database::run(shell, command).await?,
        DevCommands::Test(command) => commands::test::run(shell, command).await?,
        DevCommands::Clean(command) => commands::clean::run(shell, command)?,
        DevCommands::Snapshot(command) => commands::snapshot::run(shell, command).await?,
        DevCommands::Lint(args) => commands::lint::run(shell, args)?,
        DevCommands::Fmt(args) => commands::fmt::run(shell.clone(), args).await?,
        DevCommands::Prover(command) => commands::prover::run(shell, command).await?,
        DevCommands::Contracts(args) => commands::contracts::run(shell, args)?,
        DevCommands::ConfigWriter(args) => commands::config_writer::run(shell, args)?,
        DevCommands::SendTransactions(args) => {
            commands::send_transactions::run(shell, args).await?
        }
        DevCommands::Status(args) => commands::status::run(shell, args).await?,
        DevCommands::GenerateGenesis => commands::genesis::run(shell).await?,
    }
    Ok(())
}
