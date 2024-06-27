use clap::{Parser, Subcommand};
use commands::{database::DatabaseCommands, test::TestCommands};
use common::{
    check_prerequisites,
    config::{global_config, init_global_config, GlobalConfig},
    error::log_error,
    init_prompt_theme, logger,
};
use config::EcosystemConfig;
use messages::{
    msg_global_chain_does_not_exist, MSG_SUBCOMMAND_DATABASE_ABOUT, MSG_SUBCOMMAND_TESTS_ABOUT,
};
use xshell::Shell;

mod commands;
mod dals;
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
    #[command(subcommand, about = MSG_SUBCOMMAND_DATABASE_ABOUT)]
    Database(DatabaseCommands),
    #[command(subcommand, about = MSG_SUBCOMMAND_TESTS_ABOUT)]
    Test(TestCommands),
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
        check_prerequisites(&shell);
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
        SupervisorSubcommands::Test(command) => commands::test::run(shell, command)?,
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
