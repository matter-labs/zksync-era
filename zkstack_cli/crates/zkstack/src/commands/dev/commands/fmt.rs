use std::path::PathBuf;

use clap::Parser;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, spinner::Spinner};
use zkstack_cli_config::EcosystemConfig;

use super::sql_fmt::format_sql;
use crate::commands::dev::messages::msg_running_fmt_spinner;

async fn prettier(shell: Shell, check: bool) -> anyhow::Result<()> {
    let mode = if check { "fmt:check" } else { "fmt" };
    Ok(Cmd::new(cmd!(shell, "yarn {mode}").arg("--cache-location .prettier_cache.json")).run()?)
}

async fn prettier_contracts(shell: Shell, check: bool) -> anyhow::Result<()> {
    let prettier_command = cmd!(shell, "yarn --cwd contracts")
        .arg(format!("prettier:{}", if check { "check" } else { "fix" }))
        .arg("--cache")
        .arg("--cache-location .prettier_cache_contracts.json");
    Ok(Cmd::new(prettier_command).run()?)
}

async fn rustfmt(
    shell: Shell,
    check: bool,
    link_to_code: PathBuf,
    dir: &str,
) -> anyhow::Result<()> {
    let mode = if check { "--check " } else { "" };
    let full_path = link_to_code.join(dir).join("Cargo.toml");
    let fmt_cmd = cmd!(shell, "cargo fmt {mode}--manifest-path {full_path} --all -- --config imports_granularity=Crate --config group_imports=StdExternalCrate");
    Ok(Cmd::new(fmt_cmd).run()?)
}

#[derive(Debug, Parser)]
pub struct FmtArgs {
    #[clap(long, short = 'c')]
    pub check: bool,
}

pub async fn run(shell: Shell, args: FmtArgs) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(&shell)?;
    shell.set_var("ZKSYNC_USE_CUDA_STUBS", "true");
    let spinner = Spinner::new(&msg_running_fmt_spinner());
    let mut tasks = vec![];
    tasks.push(tokio::spawn(prettier(shell.clone(), args.check)));
    //tasks.push(tokio::spawn(format_sql(shell.clone(), args.check)));
    for dir in ["core", "prover", "zkstack_cli"] {
        tasks.push(tokio::spawn(rustfmt(
            shell.clone(),
            args.check,
            ecosystem.link_to_code.clone(),
            dir,
        )));
    }
    tasks.push(tokio::spawn(prettier_contracts(shell.clone(), args.check)));

    for result in futures::future::join_all(tasks).await {
        result??;
    }
    spinner.finish();
    Ok(())
}
