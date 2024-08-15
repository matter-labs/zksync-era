use clap::{Parser, ValueEnum};
use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use strum::EnumIter;
use xshell::{cmd, Shell};

use crate::messages::{
    msg_running_linter_for_extension_spinner, msg_running_linters_for_files,
    MSG_LINT_CONFIG_PATH_ERR, MSG_RUNNING_CONTRACTS_LINTER_SPINNER,
};

pub const IGNORED_DIRS: [&str; 18] = [
    "target",
    "node_modules",
    "volumes",
    "build",
    "dist",
    ".git",
    "generated",
    "grafonnet-lib",
    "prettier-config",
    "lint-config",
    "cache",
    "artifacts",
    "typechain",
    "binaryen",
    "system-contracts",
    "artifacts-zk",
    "cache-zk",
    // Ignore directories with OZ and forge submodules.
    "contracts/l1-contracts/lib",
];

pub const IGNORED_FILES: [&str; 4] = [
    "KeysWithPlonkVerifier.sol",
    "TokenInit.sol",
    ".tslintrc.js",
    ".prettierrc.js",
];

const CONFIG_PATH: &str = "etc/lint-config";

#[derive(Debug, Parser)]
pub struct LintArgs {
    #[clap(long, short = 'c')]
    pub check: bool,
    #[clap(long, short = 'e')]
    pub extensions: Vec<Extension>,
}

#[derive(Debug, ValueEnum, EnumIter, strum::Display, PartialEq, Eq, Clone, Copy)]
#[strum(serialize_all = "lowercase")]
pub enum Extension {
    Rs,
    Md,
    Sol,
    Js,
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
            Extension::Rs => lint_rs(shell, &ecosystem, args.check)?,
            Extension::Sol => lint_contracts(shell, &ecosystem, args.check)?,
            ext => lint(shell, &ecosystem, &ext, args.check)?,
        }
    }

    Ok(())
}

fn lint_rs(shell: &Shell, ecosystem: &EcosystemConfig, check: bool) -> anyhow::Result<()> {
    let spinner = Spinner::new(&msg_running_linter_for_extension_spinner(&Extension::Rs));

    let link_to_code = &ecosystem.link_to_code;
    let lint_to_prover = &ecosystem.link_to_code.join("prover");
    let link_to_toolbox = &ecosystem.link_to_code.join("zk_toolbox");
    let paths = vec![link_to_code, lint_to_prover, link_to_toolbox];

    spinner.freeze();
    for path in paths {
        let _dir_guard = shell.push_dir(path);
        let mut cmd = cmd!(shell, "cargo clippy");
        let common_args = &["--locked", "--", "-D warnings", "-D unstable_features"];
        if !check {
            cmd = cmd.args(&["--fix", "--allow-dirty"]);
        }
        cmd = cmd.args(common_args);
        Cmd::new(cmd).with_force_run().run()?;
    }

    Ok(())
}

fn get_linter(extension: &Extension) -> Vec<String> {
    match extension {
        Extension::Rs => vec!["cargo".to_string(), "clippy".to_string()],
        Extension::Md => vec!["markdownlint".to_string()],
        Extension::Sol => vec!["solhint".to_string()],
        Extension::Js => vec!["eslint".to_string()],
        Extension::Ts => vec!["eslint".to_string(), "--ext".to_string(), "ts".to_string()],
    }
}

fn lint(
    shell: &Shell,
    ecosystem: &EcosystemConfig,
    extension: &Extension,
    check: bool,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(&msg_running_linter_for_extension_spinner(extension));
    let _dir_guard = shell.push_dir(&ecosystem.link_to_code);
    let files = get_unignored_files(shell, extension)?;

    let cmd = cmd!(shell, "yarn");
    let config_path = ecosystem.link_to_code.join(CONFIG_PATH);
    let config_path = config_path.join(format!("{}.js", extension));
    let config_path = config_path
        .to_str()
        .expect(MSG_LINT_CONFIG_PATH_ERR)
        .to_string();

    let linter = get_linter(extension);

    let fix_option = if check {
        vec![]
    } else {
        vec!["--fix".to_string()]
    };

    let args = [
        linter.as_slice(),
        &fix_option,
        &["--config".to_string(), config_path],
        files.as_slice(),
    ]
    .concat();

    Cmd::new(cmd.args(&args)).run()?;
    spinner.finish();
    Ok(())
}

fn lint_contracts(shell: &Shell, ecosystem: &EcosystemConfig, check: bool) -> anyhow::Result<()> {
    lint(shell, ecosystem, &Extension::Sol, check)?;

    let spinner = Spinner::new(MSG_RUNNING_CONTRACTS_LINTER_SPINNER);
    let _dir_guard = shell.push_dir(&ecosystem.link_to_code);
    let cmd = cmd!(shell, "yarn");
    let linter = if check { "lint:check" } else { "lint:fix" };
    let args = ["--cwd", "contracts", linter];
    Cmd::new(cmd.args(&args)).run()?;
    spinner.finish();

    Ok(())
}

fn get_unignored_files(shell: &Shell, extension: &Extension) -> anyhow::Result<Vec<String>> {
    let mut files = Vec::new();
    let output = cmd!(shell, "git ls-files").read()?;

    for line in output.lines() {
        let path = line.to_string();
        if !IGNORED_DIRS.iter().any(|dir| path.contains(dir))
            && !IGNORED_FILES.contains(&path.as_str())
            && path.ends_with(&format!(".{}", extension))
        {
            files.push(path);
        }
    }

    Ok(files)
}
