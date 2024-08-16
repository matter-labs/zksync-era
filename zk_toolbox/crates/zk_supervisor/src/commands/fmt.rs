use std::path::PathBuf;

use clap::Parser;
use common::{cmd::Cmd, logger, spinner::Spinner};
use xshell::{cmd, Shell};

use crate::commands::lint::Extension;

const CONFIG_PATH: &str = "etc/prettier-config";

async fn prettier(shell: Shell, extension: Extension, check: bool) -> anyhow::Result<()> {
    let mode = if check { "--check" } else { "--write" };
    let glob = format!("**/*.{extension}");
    let config = format!("etc/prettier-config/{extension}.js");
    Ok(Cmd::new(cmd!(
        shell,
        "yarn --silent prettier {glob} {mode} --config {config}"
    ))
    .run()?)
}

async fn prettier_contracts(shell: Shell, check: bool) -> anyhow::Result<()> {
    let prettier_command = cmd!(shell, "yarn --silent --cwd contracts")
        .arg(format!("prettier:{}", if check { "check" } else { "fix" }));

    Ok(Cmd::new(prettier_command).run()?)
}

async fn rustfmt(shell: Shell, check: bool, link_to_code: PathBuf) -> anyhow::Result<()> {
    for dir in vec![".", "prover", "zk_toolbox"] {
        let _dir = shell.push_dir(link_to_code.join(dir));
        let mut cmd = cmd!(shell, "cargo fmt -- --config imports_granularity=Crate --config group_imports=StdExternalCrate");
        if check {
            cmd = cmd.arg("--check");
        }
        Cmd::new(cmd).run()?;
    }
    Ok(())
}

async fn run_all_rust_formatters(
    shell: Shell,
    check: bool,
    link_to_code: PathBuf,
) -> anyhow::Result<()> {
    rustfmt(shell.clone(), check, link_to_code).await?;
    format_sqlx_queries(shell, check).await?;
    Ok(())
}

#[derive(Debug, Parser)]
pub enum Formatter {
    Rustfmt,
    Contract,
    Prettier {
        #[arg(short, long)]
        extensions: Vec<Extension>,
    },
}

#[derive(Debug, Parser)]
pub struct FmtArgs {
    #[clap(long, short = 'c')]
    pub check: bool,
    #[clap(subcommand)]
    pub formatter: Option<Formatter>,
}

pub async fn run(shell: Shell, args: FmtArgs) -> anyhow::Result<()> {
    match args.formatter {
        None => {
            let mut tasks = vec![];
            let extensions: Vec<_> =
                vec![Extension::Js, Extension::Ts, Extension::Md, Extension::Sol];
            let spinner =
                Spinner::new(&format!("Running prettier for: {extensions:?} and rustfmt"));
            for ext in extensions {
                tasks.push(tokio::spawn(prettier(shell.clone(), ext, args.check)));
            }
            tasks.push(tokio::spawn(rustfmt(shell.clone(), args.check, ".".into())));
            tasks.push(tokio::spawn(format_sqlx_queries(shell.clone(), args.check)));
            tasks.push(tokio::spawn(prettier_contracts(shell.clone(), args.check)));

            futures::future::join_all(tasks)
                .await
                .iter()
                .for_each(|res| {
                    if let Err(err) = res {
                        logger::error(err)
                    }
                });
            spinner.finish()
        }
        Some(Formatter::Prettier { mut extensions }) => {
            if extensions.is_empty() {
                extensions = vec![Extension::Js, Extension::Ts, Extension::Md, Extension::Sol];
            }
            let spinner = Spinner::new(&format!("Running prettier for: {extensions:?}"));
            for ext in extensions {
                prettier(shell.clone(), ext, args.check).await?
            }
            spinner.finish()
        }
        Some(Formatter::Rustfmt) => {
            run_all_rust_formatters(shell.clone(), args.check, ".".into()).await?
        }
        Some(Formatter::Contract) => prettier_contracts(shell.clone(), args.check).await?,
    }
    Ok(())
}

async fn format_sqlx_queries(shell: Shell, check: bool) -> anyhow::Result<()> {
    // Implement your SQLx query formatting logic here.
    Ok(())
}
