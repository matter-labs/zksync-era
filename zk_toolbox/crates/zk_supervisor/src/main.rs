use clap::{Parser, Subcommand};
use commands::{
    database::DatabaseCommands, lint::LintArgs, snapshot::SnapshotCommands, test::TestCommands,
};
use common::{
    check_general_prerequisites,
    config::{global_config, init_global_config, GlobalConfig},
    error::log_error,
    init_prompt_theme, logger,
};
use config::EcosystemConfig;
use messages::{
    msg_global_chain_does_not_exist, MSG_PROVER_VERSION_ABOUT, MSG_SUBCOMMAND_CLEAN,
    MSG_SUBCOMMAND_DATABASE_ABOUT, MSG_SUBCOMMAND_FMT_ABOUT, MSG_SUBCOMMAND_LINT_ABOUT,
    MSG_SUBCOMMAND_SNAPSHOTS_CREATOR_ABOUT, MSG_SUBCOMMAND_TESTS_ABOUT,
};
use xshell::Shell;

use crate::commands::{clean::CleanCommands, fmt::FmtArgs};

mod commands;
mod dals;
mod defaults;
mod messages;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Supervisor {
    #[command(subcommand)]
    command: SupervisorSubcommands,
    #[clap(flatten)]
    global: SupervisorGlobalArgs,
}

#[derive(Subcommand, Debug)]
enum SupervisorSubcommands {
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
    #[command(hide = true)]
    Markdown,
    #[command(about = MSG_PROVER_VERSION_ABOUT)]
    ProverVersion,
}

#[derive(Parser, Debug)]
#[clap(next_help_heading = "Global options")]
struct SupervisorGlobalArgs {
    /// Verbose mode
    #[clap(short, long, global = true)]
    verbose: bool,
    /// Chain to use
    #[clap(long, global = true)]
    chain: Option<String>,
    /// Ignores prerequisites checks
    #[clap(long, global = true)]
    ignore_prerequisites: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    human_panic::setup_panic!();

    init_prompt_theme();

    logger::new_empty_line();
    logger::intro();

    let shell = Shell::new().unwrap();
    let args = Supervisor::parse();

    init_global_config_inner(&shell, &args.global)?;

    if !global_config().ignore_prerequisites {
        check_general_prerequisites(&shell);
    }

    match run_subcommand(args, &shell).await {
        Ok(_) => {}
        Err(error) => {
            log_error(error);
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn run_subcommand(args: Supervisor, shell: &Shell) -> anyhow::Result<()> {
    match args.command {
        SupervisorSubcommands::Database(command) => commands::database::run(shell, command).await?,
        SupervisorSubcommands::Test(command) => commands::test::run(shell, command).await?,
        SupervisorSubcommands::Clean(command) => commands::clean::run(shell, command)?,
        SupervisorSubcommands::Snapshot(command) => commands::snapshot::run(shell, command).await?,
        SupervisorSubcommands::Markdown => {
            clap_markdown::print_help_markdown::<Supervisor>();
        }
        SupervisorSubcommands::Lint(args) => commands::lint::run(shell, args)?,
        SupervisorSubcommands::Fmt(args) => commands::fmt::run(shell.clone(), args).await?,
        SupervisorSubcommands::ProverVersion => commands::prover_version::run(shell).await?,
    }
    Ok(())
}

fn init_global_config_inner(shell: &Shell, args: &SupervisorGlobalArgs) -> anyhow::Result<()> {
    if let Some(name) = &args.chain {
        if let Ok(config) = EcosystemConfig::from_file(shell) {
            let chains = config.list_of_chains();
            if !chains.contains(name) {
                anyhow::bail!(msg_global_chain_does_not_exist(name, &chains.join(", ")));
            }
        }
    }

    init_global_config(GlobalConfig {
        verbose: args.verbose,
        chain_name: args.chain.clone(),
        ignore_prerequisites: args.ignore_prerequisites,
    });
    Ok(())
}
