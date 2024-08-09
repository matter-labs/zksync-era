use clap::{Parser, ValueEnum};
use common::{cmd::Cmd, logger};
use config::EcosystemConfig;
use strum::EnumIter;
use xshell::{cmd, Shell};

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
    let extensions = if args.extensions.is_empty() {
        vec![
            Extension::Rs,
            Extension::Md,
            Extension::Sol,
            Extension::Js,
            Extension::Ts,
        ]
    } else {
        args.extensions.clone()
    };

    logger::info(msg_running_linters_for_files(&extensions));

    let ecosystem = EcosystemConfig::from_file(shell)?;

    for extension in extensions {
        match extension {
            Extension::Rs => linter_rs(shell, &ecosystem)?,
            Extension::Md => linter_md(shell, &ecosystem)?,
            Extension::Sol => linter_sol(shell, &ecosystem, args.check)?,
            Extension::Js => linter_js(shell, &ecosystem)?,
            Extension::Ts => linter_ts(shell, &ecosystem)?,
        }
    }

    Ok(())
}

fn linter_rs(shell: &Shell, ecosystem: &EcosystemConfig) -> anyhow::Result<()> {
    let link_to_code = &ecosystem.link_to_code;
    let lint_to_prover = &ecosystem.link_to_code.join("prover");
    let link_to_toolbox = &ecosystem.link_to_code.join("zk_toolbox");
    let paths = vec![link_to_code, lint_to_prover, link_to_toolbox];

    for path in paths {
        let _dir_guard = shell.push_dir(path);
        Cmd::new(cmd!(
            shell,
            "cargo clippy --locked -- -D warnings -D unstable_features"
        ))
        .with_force_run()
        .run()?;
    }

    Ok(())
}

fn linter_md(shell: &Shell, ecosystem: &EcosystemConfig) -> anyhow::Result<()> {
    todo!()
}

fn linter_sol(shell: &Shell, ecosystem: &EcosystemConfig, check: bool) -> anyhow::Result<()> {
    todo!()
}

fn linter_js(shell: &Shell, ecosystem: &EcosystemConfig) -> anyhow::Result<()> {
    todo!()
}

fn linter_ts(shell: &Shell, ecosystem: &EcosystemConfig) -> anyhow::Result<()> {
    todo!()
}
