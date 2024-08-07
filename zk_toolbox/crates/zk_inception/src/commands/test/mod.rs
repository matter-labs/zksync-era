use clap::Subcommand;
use xshell::Shell;

mod rust;

#[derive(Subcommand, Debug)]
pub enum TestCommands {
    /// Run unit-tests. Runs all tests in all rust bins and libs by default. Accepts optional arbitrary cargo test flags.
    Rust,
}

pub fn run(shell: &Shell, args: TestCommands) -> anyhow::Result<()> {
    match args {
        TestCommands::Rust => rust::run(shell),
    }
}
