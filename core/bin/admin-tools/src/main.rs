use std::convert::TryFrom;

use application::{App, AppError};
use blocks::print_block_info;
use clap::{Args, Parser, Subcommand};
use zksync_dal::prover_dal::GetProverJobsParams;
use zksync_types::proofs::AggregationRound;
use zksync_types::L1BatchNumber;

use crate::application::create_app;

mod application;
mod blocks;
mod prover;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum JobStatus {
    Queued,
    Failed,
    InProgress,
    Successful,
}

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[arg(long)]
    /// Can be used to load environment from ./etc/env/<profile>.env file.
    profile: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    #[command(subcommand)]
    Prover(ProverCommand),
    #[command(subcommand)]
    Blocks(BlockCommand),
}

#[derive(Subcommand)]
enum ProverCommand {
    /// Show general prover jobs statistics.
    Stats,
    /// List specific jobs
    Ls(ProverLsCommand),
}

#[derive(Subcommand)]
enum BlockCommand {
    Show(BlockShowCommand),
}

type AppFnBox<'a> = Box<dyn FnOnce(&mut App) -> Result<(), AppError> + 'a>;
type CmdMatch<'a> = Result<AppFnBox<'a>, AppError>;

fn prover_stats<'a>() -> AppFnBox<'a> {
    Box::new(|app| {
        let stats = prover::get_stats(app)?;
        prover::print_stats(&stats, app.terminal.width)
    })
}

#[derive(Args)]
struct ProverLsCommand {
    #[arg(long, short)]
    /// Statuses to include.
    status: Option<Vec<JobStatus>>,
    #[arg(long, short, default_value_t = 10)]
    /// Limits the amount of returned results.
    limit: u32,
    #[arg(long)]
    desc: bool,
    #[arg(long)]
    /// Block range. Format: `x` or `x..y`.
    range: Option<String>,
    #[arg(long)]
    round: Option<i32>,
}

fn prover_ls<'a>(cmd: &ProverLsCommand) -> Result<AppFnBox<'a>, AppError> {
    let range = match &cmd.range {
        Some(input) => {
            let split = input
                .split("..")
                .map(|x| x.parse::<u32>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| AppError::Command("Wrong range format".to_owned()))?;

            match split.as_slice() {
                [] => Ok(None),
                [id] => Ok(Some(L1BatchNumber(*id)..L1BatchNumber(*id))),
                [s, e] => Ok(Some(L1BatchNumber(*s)..L1BatchNumber(*e))),
                _ => Err(AppError::Command("Wrong range format".to_owned())),
            }
        }
        None => Ok(None),
    }?;

    let opts = GetProverJobsParams {
        blocks: range,
        statuses: cmd.status.as_ref().map(|x| {
            x.iter()
                .map(|x| {
                    clap::ValueEnum::to_possible_value(x)
                        .unwrap()
                        .get_name()
                        .replace('-', "_")
                })
                .collect()
        }),
        limit: Some(cmd.limit),
        desc: cmd.desc,
        round: cmd
            .round
            .map_or(Ok(None), |x| AggregationRound::try_from(x).map(Some))
            .map_err(|_| AppError::Command("Wrong aggregation round value.".to_owned()))?,
    };

    Ok(Box::new(move |app| {
        let jobs = prover::get_jobs(app, opts)?;
        prover::print_jobs(&jobs)
    }))
}

#[derive(Args)]
struct BlockShowCommand {
    id: u32,
}

fn block_show<'a>(id: L1BatchNumber) -> AppFnBox<'a> {
    Box::new(move |app| {
        let block = blocks::get_block_info(id, app)?;
        print_block_info(&block)
    })
}

fn match_prover_cmd(cmd: &ProverCommand) -> CmdMatch {
    match cmd {
        ProverCommand::Stats => Ok(prover_stats()),
        ProverCommand::Ls(x) => prover_ls(x),
    }
}

fn match_block_cmd(cmd: &BlockCommand) -> CmdMatch {
    match cmd {
        BlockCommand::Show(cmd) => Ok(block_show(L1BatchNumber(cmd.id))),
    }
}

fn match_cmd(cmd: &Command) -> CmdMatch {
    match cmd {
        Command::Prover(cmd) => match_prover_cmd(cmd),
        Command::Blocks(cmd) => match_block_cmd(cmd),
    }
}

fn main() -> Result<(), AppError> {
    let cli = Cli::parse();

    let exec_fn = match_cmd(&cli.command)?;

    let mut app = create_app(&cli.profile)?;

    println!();
    let result = exec_fn(&mut app);
    println!();

    result
}
