use clap::Subcommand;
use commands::{
    rich_account::args::RichAccountArgs, status::args::StatusArgs,
    track_priority_txs::TrackPriorityOpsArgs,
};
#[cfg(feature = "v27_evm_interpreter")]
use messages::MSG_V27_EVM_INTERPRETER_UPGRADE;
#[cfg(feature = "v28_precompiles")]
use messages::MSG_V28_PRECOMPILES_UPGRADE;
use messages::{MSG_RICH_ACCOUNT_ABOUT, MSG_STATUS_ABOUT};
use xshell::Shell;

use self::commands::{
    clean::CleanCommands, config_writer::ConfigWriterArgs, contracts::ContractsArgs,
    database::DatabaseCommands, fmt::FmtArgs, init_test_wallet::run as init_test_wallet_run,
    lint::LintArgs, prover::ProverCommands, send_transactions::args::SendTransactionsArgs,
    snapshot::SnapshotCommands, test::TestCommands,
};
use crate::commands::dev::messages::{
    MSG_CONFIG_WRITER_ABOUT, MSG_CONTRACTS_ABOUT, MSG_GENERATE_GENESIS_ABOUT,
    MSG_INIT_TEST_WALLET_ABOUT, MSG_PROVER_VERSION_ABOUT, MSG_SEND_TXNS_ABOUT,
    MSG_SUBCOMMAND_CLEAN, MSG_SUBCOMMAND_DATABASE_ABOUT, MSG_SUBCOMMAND_FMT_ABOUT,
    MSG_SUBCOMMAND_LINT_ABOUT, MSG_SUBCOMMAND_SNAPSHOTS_CREATOR_ABOUT, MSG_SUBCOMMAND_TESTS_ABOUT,
};
#[cfg(feature = "v29_interopA_ff")]
use crate::commands::dev::messages::{
    MSG_V29_INTEROP_A_FF_CHAIN_UPGRADE, MSG_V29_INTEROP_A_FF_UPGRADE,
};

pub(crate) mod commands;
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
    #[command(about = MSG_INIT_TEST_WALLET_ABOUT)]
    InitTestWallet,
    #[command(about = MSG_RICH_ACCOUNT_ABOUT)]
    RichAccount(RichAccountArgs),
    #[command(about = MSG_GENERATE_GENESIS_ABOUT)]
    TrackPriorityOps(TrackPriorityOpsArgs),
    #[cfg(feature = "v27_evm_interpreter")]
    #[command(about = MSG_V27_EVM_INTERPRETER_UPGRADE)]
    V27EvmInterpreterUpgradeCalldata(commands::v27_evm_eq::V27EvmInterpreterCalldataArgs),
    #[cfg(feature = "v28_precompiles")]
    #[command(about = MSG_V28_PRECOMPILES_UPGRADE)]
    GenerateV28UpgradeCalldata(commands::v28_precompiles::V28PrecompilesCalldataArgs),
    #[cfg(feature = "v29_interopA_ff")]
    #[command(about = MSG_V29_INTEROP_A_FF_UPGRADE)]
    GenerateV29EcosystemCalldata(commands::v29_ecosystem_args::EcosystemUpgradeArgs),
    #[cfg(feature = "v29_interopA_ff")]
    #[command(about = MSG_V29_INTEROP_A_FF_UPGRADE)]
    RunV29EcosystemUpgrade(commands::v29_ecosystem_args::EcosystemUpgradeArgs),
    #[cfg(feature = "v29_interopA_ff")]
    #[command(about = MSG_V29_INTEROP_A_FF_CHAIN_UPGRADE)]
    GenerateV29ChainUpgrade(commands::v29_chain_args::V29ChainUpgradeArgs),
    #[cfg(feature = "v29_interopA_ff")]
    #[command(about = MSG_V29_INTEROP_A_FF_CHAIN_UPGRADE)]
    RunV29ChainUpgrade(commands::v29_chain_args::V29ChainUpgradeArgs),
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
        DevCommands::InitTestWallet => init_test_wallet_run(shell).await?,
        DevCommands::RichAccount(args) => commands::rich_account::run(shell, args).await?,
        DevCommands::TrackPriorityOps(args) => commands::track_priority_txs::run(args).await?,
        #[cfg(feature = "v27_evm_interpreter")]
        DevCommands::V27EvmInterpreterUpgradeCalldata(args) => {
            commands::v27_evm_eq::run(shell, args).await?
        }
        #[cfg(feature = "v28_precompiles")]
        DevCommands::GenerateV28UpgradeCalldata(args) => {
            commands::v28_precompiles::run(shell, args).await?
        }
        #[cfg(feature = "v29_interopA_ff")]
        DevCommands::GenerateV29EcosystemCalldata(args) => {
            commands::v29_ecosystem_upgrade::run(shell, args, false).await?
        }
        #[cfg(feature = "v29_interopA_ff")]
        DevCommands::RunV29EcosystemUpgrade(args) => {
            commands::v29_ecosystem_upgrade::run(shell, args, true).await?
        }
        #[cfg(feature = "v29_interopA_ff")]
        DevCommands::GenerateV29ChainUpgrade(args) => {
            commands::v29_chain_upgrade::run(shell, args, false).await?
        }
        #[cfg(feature = "v29_interopA_ff")]
        DevCommands::RunV29ChainUpgrade(args) => {
            commands::v29_chain_upgrade::run(shell, args, true).await?
        }
    }
    Ok(())
}
