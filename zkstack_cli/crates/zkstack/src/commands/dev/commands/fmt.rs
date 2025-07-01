use std::path::PathBuf;

use clap::Parser;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, spinner::Spinner};
use zkstack_cli_config::EcosystemConfig;

use super::sql_fmt::format_sql;
use crate::commands::dev::{commands::lint_utils::Target, messages::msg_running_fmt_spinner};

async fn prettier(shell: Shell, check: bool) -> anyhow::Result<()> {
    let mode = if check { "fmt:check" } else { "fmt" };
    let mut command = cmd!(shell, "yarn {mode}").args(["--cache-location", ".prettier_cache.json"]);
    if !check {
        command = command.args(["--log-level", "silent"]);
    }
    Ok(Cmd::new(command).run()?)
}

async fn prettier_contracts(shell: Shell, check: bool) -> anyhow::Result<()> {
    let mut prettier_command = cmd!(shell, "yarn --cwd contracts")
        .arg(format!("prettier:{}", if check { "check" } else { "fix" }))
        .arg("--cache")
        .args(["--cache-location", "../.prettier_cache_contracts.json"]);

    if !check {
        prettier_command = prettier_command.args(["--log-level", "silent"]);
    }
    Ok(Cmd::new(prettier_command).run()?)
}

async fn rustfmt(
    shell: Shell,
    check: bool,
    link_to_code: PathBuf,
    dir: &str,
) -> anyhow::Result<()> {
    let full_path = link_to_code.join(dir).join("Cargo.toml");
    let mut fmt_cmd = cmd!(shell, "cargo fmt --manifest-path {full_path}")
        .arg("--all")
        .arg("--")
        .args(["--config", "imports_granularity=Crate"])
        .args(["--config", "group_imports=StdExternalCrate"]);
    if check {
        fmt_cmd = fmt_cmd.arg("--check");
    }
    Ok(Cmd::new(fmt_cmd).run()?)
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum Mode {
    Rust,
    Prettier,
    PrettierContracts,
}

#[derive(Debug, Parser)]
pub struct FmtArgs {
    #[clap(long, short = 'c')]
    pub check: bool,
    #[clap(value_enum)]
    pub mode: Option<Mode>,
}

pub async fn run(shell: Shell, args: FmtArgs) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(&shell)?;
    shell.set_var("ZKSYNC_USE_CUDA_STUBS", "true");
    let mut tasks = vec![];

    let mut targets = vec![];
    if matches!(args.mode, Some(Mode::Rust) | None) {
        // Run Rust formatting
        for dir in ["core", "prover", "zkstack_cli"] {
            tasks.push(tokio::spawn(rustfmt(
                shell.clone(),
                args.check,
                ecosystem.link_to_code.clone(),
                dir,
            )));
        }
        tasks.push(tokio::spawn(format_sql(shell.clone(), args.check)));
        targets.push(Target::Rs);
    }

    if matches!(args.mode, Some(Mode::Prettier) | None) {
        // Run prettier (not contracts)
        tasks.push(tokio::spawn(prettier(shell.clone(), args.check)));
        targets.append(&mut vec![Target::Js, Target::Ts, Target::Sol, Target::Md])
    }

    if matches!(args.mode, Some(Mode::PrettierContracts) | None) {
        // Run prettier for contracts
        tasks.push(tokio::spawn(prettier_contracts(shell.clone(), args.check)));
        targets.push(Target::Contracts)
    }

    let spinner = Spinner::new(&msg_running_fmt_spinner(&targets));

    for result in futures::future::join_all(tasks).await {
        result??;
    }
    spinner.finish();
    Ok(())
}
