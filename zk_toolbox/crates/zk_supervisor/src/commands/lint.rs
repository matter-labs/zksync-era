use clap::{Parser, ValueEnum};
use common::logger;
use strum::EnumIter;
use xshell::Shell;

use crate::messages::msg_running_linters_for_files;

#[derive(Debug, Parser)]
pub struct LintArgs {
    #[clap(long, short = 'c')]
    pub check: bool,
    #[clap(long, short = 'e')]
    pub extensions: Vec<Extension>,
}

#[derive(Debug, ValueEnum, EnumIter, strum::Display, PartialEq, Eq, Clone)]
pub enum Extension {
    #[strum(serialize = "rs")]
    Rs,
    #[strum(serialize = "md")]
    Md,
    #[strum(serialize = "sol")]
    Sol,
    #[strum(serialize = "js")]
    Js,
    #[strum(serialize = "ts")]
    Ts,
}

pub fn run(shell: &Shell, args: LintArgs) -> anyhow::Result<()> {
    logger::info(&msg_running_linters_for_files(&args.extensions));
    Ok(())
}
