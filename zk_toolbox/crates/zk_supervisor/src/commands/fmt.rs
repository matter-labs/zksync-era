use std::env::args;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

use clap::Parser;
use common::{cmd::Cmd, logger};
use xshell::{cmd, Shell};

use crate::commands::lint::{Extension, IGNORED_DIRS, IGNORED_FILES};

const CONFIG_PATH: &str = "etc/prettier-config";

fn prettier(shell: &Shell, extension: Extension, check: bool) -> anyhow::Result<()> {
    let command = if check { "check" } else { "write" };
    let files = get_unignored_files(shell, extension)?;
    logger::info(format!("Got {} files for {extension}", files.len()));

    if files.is_empty() {
        logger::info(format!("No files of extension {extension} to format"));
        return Ok(());
    }

    let mut prettier_command = cmd!(shell, "yarn --silent prettier --config ")
        .arg(format!("{}/{}.js", CONFIG_PATH, extension))
        .arg(format!("--{}", command))
        .arg(format!("{}", files.join(" ")));

    // if !check {
    //     prettier_command = prettier_command.arg("> /dev/null");
    // }

    Ok(prettier_command.run()?)
}

fn prettier_contracts(shell: &Shell, check: bool) -> anyhow::Result<()> {
    let mut prettier_command = cmd!(shell, "yarn --silent --cwd contracts")
        .arg(format!("prettier:{}", if check { "check" } else { "fix" }));

    // if !check {
    //     prettier_command = prettier_command.arg("> /dev/null");
    // }

    Ok(Cmd::new(prettier_command).run()?)
}

fn rustfmt(shell: &Shell, check: bool, link_to_code: PathBuf) -> anyhow::Result<()> {
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

fn run_all_rust_formatters(
    shell: &Shell,
    check: bool,
    link_to_code: PathBuf,
) -> anyhow::Result<()> {
    rustfmt(shell, check, link_to_code)?;
    format_sqlx_queries(check)?;
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

pub fn run(shell: &Shell, args: FmtArgs) -> anyhow::Result<()> {
    match args.formatter {
        None => {
            let extensions: Vec<_> =
                vec![Extension::Js, Extension::Ts, Extension::Md, Extension::Sol];
            for ext in extensions {
                prettier(shell, ext, args.check)?
            }
            run_all_rust_formatters(shell, args.check, ".".into())?;
            prettier_contracts(shell, args.check)?
        }
        Some(Formatter::Prettier { mut extensions }) => {
            if extensions.is_empty() {
                extensions = vec![Extension::Js, Extension::Ts, Extension::Md, Extension::Sol];
            }
            for ext in extensions {
                prettier(shell, ext, args.check)?
            }
        }
        Some(Formatter::Rustfmt) => run_all_rust_formatters(shell, args.check, ".".into())?,
        Some(Formatter::Contract) => prettier_contracts(shell, args.check)?,
    }
    Ok(())
}

fn get_unignored_files(shell: &Shell, extension: Extension) -> anyhow::Result<Vec<String>> {
    let root = if let Extension::Sol = extension {
        "contracts"
    } else {
        "."
    };

    let ignored_dirs: Vec<_> = IGNORED_DIRS
        .iter()
        .map(|dir| {
            vec![
                "-o".to_string(),
                "-path".to_string(),
                format!("'*{dir}'"),
                "-prune".to_string(),
            ]
        })
        .flatten()
        .collect();

    let ignored_files: Vec<_> = IGNORED_FILES
        .iter()
        .map(|file| {
            vec![
                "-a".to_string(),
                "!".to_string(),
                "-name".to_ascii_lowercase(),
                format!("'{file}'"),
            ]
        })
        .flatten()
        .collect();

    dbg!(shell.current_dir());
    // let output = Cmd::new(
    //     cmd!(shell, "find {root} -type f -name").arg(format!("'*.{extension}'")), // .args(ignored_files)
    //                                                                               // .arg("-print"),
    // )
    // .run_with_output()?;
    // dbg!(&output);
    let output = Cmd::new(
        cmd!(shell, "find {root} -type f -name '*.js'")
            .args(ignored_files)
            .arg("-print")
            .args(ignored_dirs),
    )
        .run_with_output()?;
    dbg!(&output);

    let files = String::from_utf8(output.stdout)?
        .lines()
        .map(|line| line.to_string())
        .collect();

    Ok(files)
}

fn format_sqlx_queries(check: bool) -> anyhow::Result<()> {
    // Implement your SQLx query formatting logic here.
    Ok(())
}
