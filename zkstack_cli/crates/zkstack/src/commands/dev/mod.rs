use clap::Subcommand;
use commands::{
    rich_account::args::RichAccountArgs, status::args::StatusArgs,
    track_priority_txs::TrackPriorityOpsArgs,
};
use messages::{
    MSG_RICH_ACCOUNT_ABOUT, MSG_STATUS_ABOUT, MSG_V27_EVM_INTERPRETER_UPGRADE,
    MSG_V28_PRECOMPILES_UPGRADE,
};
use xshell::Shell;

use self::commands::{
    clean::CleanCommands, config_writer::ConfigWriterArgs, contracts::ContractsArgs,
    database::DatabaseCommands, fmt::FmtArgs, init_test_wallet::run as init_test_wallet_run,
    lint::LintArgs, prover::ProverCommands, send_transactions::args::SendTransactionsArgs,
    snapshot::SnapshotCommands, test::TestCommands,
};
use crate::commands::dev::messages::{
    GENERAL_CHAIN_UPGRADE, GENERAL_ECOSYSTEM_UPGRADE, MSG_CONFIG_WRITER_ABOUT, MSG_CONTRACTS_ABOUT,
    MSG_GENERATE_GENESIS_ABOUT, MSG_INIT_TEST_WALLET_ABOUT, MSG_PROVER_VERSION_ABOUT,
    MSG_SEND_TXNS_ABOUT, MSG_SUBCOMMAND_CLEAN, MSG_SUBCOMMAND_DATABASE_ABOUT,
    MSG_SUBCOMMAND_FMT_ABOUT, MSG_SUBCOMMAND_LINT_ABOUT, MSG_SUBCOMMAND_SNAPSHOTS_CREATOR_ABOUT,
    MSG_SUBCOMMAND_TESTS_ABOUT, V29_CHAIN_UPGRADE,
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
    #[command(about = MSG_V27_EVM_INTERPRETER_UPGRADE)]
    V27EvmInterpreterUpgradeCalldata(commands::upgrades::v27_evm_eq::V27EvmInterpreterCalldataArgs),
    #[command(about = MSG_V28_PRECOMPILES_UPGRADE)]
    GenerateV28UpgradeCalldata(commands::upgrades::v28_precompiles::V28PrecompilesCalldataArgs),
    #[command(about = GENERAL_ECOSYSTEM_UPGRADE)]
    GenerateEcosystemUpgradeCalldata(commands::upgrades::args::ecosystem::EcosystemUpgradeArgs),
    #[command(about = GENERAL_ECOSYSTEM_UPGRADE)]
    RunEcosystemUpgrade(commands::upgrades::args::ecosystem::EcosystemUpgradeArgs),
    #[command(about = GENERAL_CHAIN_UPGRADE)]
    GenerateChainUpgrade(commands::upgrades::args::chain::DefaultChainUpgradeArgs),
    #[command(about = GENERAL_CHAIN_UPGRADE)]
    RunChainUpgrade(commands::upgrades::args::chain::DefaultChainUpgradeArgs),
    #[command(about = V29_CHAIN_UPGRADE)]
    RunV29ChainUpgrade(commands::upgrades::args::v29_chain::V29ChainUpgradeArgs),
    #[command(about = V29_CHAIN_UPGRADE)]
    GenerateV29ChainUpgrade(commands::upgrades::args::v29_chain::V29ChainUpgradeArgs),
    #[command(about = V29_CHAIN_UPGRADE)]
    RunV30ZZksyncOSChainUpgrade(commands::upgrades::args::v30_chain::V30ChainUpgradeArgs),
    #[command(about = V29_CHAIN_UPGRADE)]
    GenerateV30ZksyncOSChainUpgrade(commands::upgrades::args::v30_chain::V30ChainUpgradeArgs),
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

        DevCommands::V27EvmInterpreterUpgradeCalldata(args) => {
            commands::upgrades::v27_evm_eq::run(shell, args).await?
        }
        DevCommands::GenerateV28UpgradeCalldata(args) => {
            commands::upgrades::v28_precompiles::run(shell, args).await?
        }
        DevCommands::GenerateEcosystemUpgradeCalldata(args) => {
            commands::upgrades::default_ecosystem_upgrade::run(shell, args, false).await?
        }
        DevCommands::RunEcosystemUpgrade(args) => {
            commands::upgrades::default_ecosystem_upgrade::run(shell, args, true).await?
        }
        DevCommands::GenerateChainUpgrade(args) => {
            commands::upgrades::default_chain_upgrade::run(shell, args, false).await?
        }
        DevCommands::RunChainUpgrade(args) => {
            commands::upgrades::default_chain_upgrade::run(shell, args, true).await?
        }
        DevCommands::GenerateV29ChainUpgrade(args) => {
            commands::upgrades::v29_upgrade::run(shell, args, false).await?
        }
        DevCommands::RunV29ChainUpgrade(args) => {
            commands::upgrades::v29_upgrade::run(shell, args, true).await?
        }
        DevCommands::GenerateV30ZksyncOSChainUpgrade(args) => {
            commands::upgrades::v30_upgrade::run(shell, args, false).await?
        }
        DevCommands::RunV30ZZksyncOSChainUpgrade(args) => {
            commands::upgrades::v30_upgrade::run(shell, args, true).await?
        }
    }
    Ok(())
}
