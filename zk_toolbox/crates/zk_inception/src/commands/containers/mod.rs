use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;

mod down;
mod reset;
mod up;

pub use up::docker_up;

#[derive(Debug, Serialize, Deserialize, Parser)]
pub enum ContainersArgs {
    Up,
    Down,
    Reset,
}

pub(crate) fn run(shell: &Shell, args: ContainersArgs) -> anyhow::Result<()> {
    match args {
        ContainersArgs::Up => up::run(shell),
        ContainersArgs::Down => down::run(shell),
        ContainersArgs::Reset => reset::run(shell),
    }
}
