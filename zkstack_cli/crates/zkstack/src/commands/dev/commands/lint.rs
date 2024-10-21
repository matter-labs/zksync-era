use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use anyhow::{bail, Context};
use clap::Parser;
use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::commands::{
    autocomplete::{autocomplete_file_name, generate_completions},
    dev::{
        commands::lint_utils::{get_unignored_files, Target},
        messages::{
            msg_running_linter_for_extension_spinner, msg_running_linters_for_files,
            MSG_LINT_CONFIG_PATH_ERR, MSG_RUNNING_CONTRACTS_LINTER_SPINNER,
        },
    },
};

const CONFIG_PATH: &str = "etc/lint-config";

#[derive(Debug, Parser)]
pub struct LintArgs {
    #[clap(long, short = 'c')]
    pub check: bool,
    #[clap(long, short = 't')]
    pub targets: Vec<Target>,
}

pub fn run(shell: &Shell, args: LintArgs) -> anyhow::Result<()> {
    let targets = if args.targets.is_empty() {
        vec![
            Target::Rs,
            Target::Md,
            Target::Sol,
            Target::Js,
            Target::Ts,
            Target::Contracts,
            Target::Autocompletion,
        ]
    } else {
        args.targets.clone()
    };

    logger::info(msg_running_linters_for_files(&targets));

    let ecosystem = EcosystemConfig::from_file(shell)?;

    for target in targets {
        match target {
            Target::Rs => lint_rs(shell, &ecosystem, args.check)?,
            Target::Contracts => lint_contracts(shell, &ecosystem, args.check)?,
            Target::Autocompletion => lint_autocompletion_files(shell, args.check)?,
            ext => lint(shell, &ecosystem, &ext, args.check)?,
        }
    }

    logger::outro("Linting complete.");

    Ok(())
}

fn lint_rs(shell: &Shell, ecosystem: &EcosystemConfig, check: bool) -> anyhow::Result<()> {
    let spinner = Spinner::new(&msg_running_linter_for_extension_spinner(&Target::Rs));

    let link_to_code = &ecosystem.link_to_code;
    let lint_to_prover = &ecosystem.link_to_code.join("prover");
    let link_to_zkstack = &ecosystem.link_to_code.join("zkstack_cli");
    let paths = vec![link_to_code, lint_to_prover, link_to_zkstack];

    spinner.freeze();
    for path in paths {
        let _dir_guard = shell.push_dir(path);
        let mut cmd = cmd!(shell, "cargo clippy");
        let common_args = &["--locked", "--", "-D", "warnings"];
        if !check {
            cmd = cmd.args(&["--fix", "--allow-dirty"]);
        }
        cmd = cmd.args(common_args);
        Cmd::new(cmd).with_force_run().run()?;
    }

    Ok(())
}

fn get_linter(target: &Target) -> Vec<String> {
    match target {
        Target::Rs => vec!["cargo".to_string(), "clippy".to_string()],
        Target::Md => vec!["markdownlint".to_string()],
        Target::Sol => vec!["solhint".to_string()],
        Target::Js => vec!["eslint".to_string()],
        Target::Ts => vec!["eslint".to_string(), "--ext".to_string(), "ts".to_string()],
        Target::Contracts => vec![],
        Target::Autocompletion => vec![],
    }
}

fn lint(
    shell: &Shell,
    ecosystem: &EcosystemConfig,
    target: &Target,
    check: bool,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(&msg_running_linter_for_extension_spinner(target));
    let _dir_guard = shell.push_dir(&ecosystem.link_to_code);
    let files = get_unignored_files(shell, target, None)?;
    let cmd = cmd!(shell, "yarn");
    let config_path = ecosystem.link_to_code.join(CONFIG_PATH);
    let config_path = config_path.join(format!("{}.js", target));
    let config_path = config_path
        .to_str()
        .expect(MSG_LINT_CONFIG_PATH_ERR)
        .to_string();

    let linter = get_linter(target);

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
    let spinner = Spinner::new(MSG_RUNNING_CONTRACTS_LINTER_SPINNER);
    let _dir_guard = shell.push_dir(&ecosystem.link_to_code);
    let cmd = cmd!(shell, "yarn");
    let linter = if check { "lint:check" } else { "lint:fix" };
    let args = ["--cwd", "contracts", linter];
    Cmd::new(cmd.args(&args)).run()?;
    spinner.finish();

    Ok(())
}

fn lint_autocompletion_files(_shell: &Shell, check: bool) -> anyhow::Result<()> {
    let completion_folder = Path::new("./zkstack_cli/crates/zkstack/completion/");
    if !completion_folder.exists() {
        logger::info("WARNING: Please run this command from the project's root folder");
        return Ok(());
    }

    // Array of supported shells
    let shells = [
        clap_complete::Shell::Bash,
        clap_complete::Shell::Fish,
        clap_complete::Shell::Zsh,
    ];

    for shell in shells {
        let mut writer = Vec::new();

        generate_completions(shell, &mut writer)
            .context("Failed to generate autocompletion file")?;

        let new = String::from_utf8(writer)?;

        let path = completion_folder.join(autocomplete_file_name(&shell));
        let mut autocomplete_file = File::open(path.clone())
            .context(format!("failed to open {}", autocomplete_file_name(&shell)))?;

        let mut old = String::new();
        autocomplete_file.read_to_string(&mut old)?;

        if new != old {
            if !check {
                let mut autocomplete_file = File::create(path).context("Failed to create file")?;
                autocomplete_file.write_all(new.as_bytes())?;
            } else {
                bail!("Autocompletion files need to be regenerated. Run `zkstack dev lint -t autocompletion` to fix this issue.")
            }
        }
    }

    Ok(())
}
