use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::bail;
use strum::{EnumIter, IntoEnumIterator};
use xshell::{cmd, Shell};
use zkstack_cli_common::{
    cmd::Cmd, git, logger, spinner::Spinner, Prompt, PromptConfirm, PromptSelect,
};
use zkstack_cli_config::ZKSYNC_ERA_GIT_REPO;

use crate::messages::{
    msg_path_to_zksync_does_not_exist_err, MSG_CLONING_ERA_REPO_SPINNER,
    MSG_CONFIRM_STILL_USE_FOLDER, MSG_LINK_TO_CODE_PROMPT, MSG_LINK_TO_CODE_SELECTION_CLONE,
    MSG_LINK_TO_CODE_SELECTION_PATH, MSG_NOT_MAIN_REPO_OR_FORK_ERR, MSG_REPOSITORY_ORIGIN_PROMPT,
};

#[derive(Debug, Clone, EnumIter, PartialEq, Eq)]
enum LinkToCodeSelection {
    Clone,
    Path,
}

impl std::fmt::Display for LinkToCodeSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkToCodeSelection::Clone => write!(f, "{MSG_LINK_TO_CODE_SELECTION_CLONE}"),
            LinkToCodeSelection::Path => write!(f, "{MSG_LINK_TO_CODE_SELECTION_PATH}"),
        }
    }
}

fn check_link_to_code(shell: &Shell, path: &str) -> anyhow::Result<()> {
    let path = Path::new(path);
    if !shell.path_exists(path) {
        bail!(msg_path_to_zksync_does_not_exist_err(
            path.to_str().unwrap()
        ));
    }

    let _guard = shell.push_dir(path);
    let out = String::from_utf8(
        Cmd::new(cmd!(shell, "git remote -v"))
            .run_with_output()?
            .stdout,
    )?;

    if !out.contains("matter-labs/zksync-era") {
        bail!(MSG_NOT_MAIN_REPO_OR_FORK_ERR);
    }

    Ok(())
}

fn pick_new_link_to_code(shell: &Shell) -> String {
    let link_to_code: String = Prompt::new(MSG_LINK_TO_CODE_PROMPT).ask();
    match check_link_to_code(shell, &link_to_code) {
        Ok(_) => link_to_code,
        Err(err) => {
            logger::warn(err);
            if !PromptConfirm::new(MSG_CONFIRM_STILL_USE_FOLDER).ask() {
                pick_new_link_to_code(shell)
            } else {
                link_to_code
            }
        }
    }
}

pub(crate) fn get_link_to_code(shell: &Shell) -> String {
    let link_to_code_selection =
        PromptSelect::new(MSG_REPOSITORY_ORIGIN_PROMPT, LinkToCodeSelection::iter()).ask();
    match link_to_code_selection {
        LinkToCodeSelection::Clone => "".to_string(),
        LinkToCodeSelection::Path => {
            let mut path: String = Prompt::new(MSG_LINK_TO_CODE_PROMPT).ask();
            if let Err(err) = check_link_to_code(shell, &path) {
                logger::warn(err);
                if !PromptConfirm::new(MSG_CONFIRM_STILL_USE_FOLDER).ask() {
                    path = pick_new_link_to_code(shell);
                }
            }
            path
        }
    }
}

pub(crate) fn resolve_link_to_code(
    shell: &Shell,
    base_path: &Path,
    link_to_code: String,
    update_submodules: Option<bool>,
) -> anyhow::Result<PathBuf> {
    if link_to_code.is_empty() {
        if base_path.join("zksync-era").exists() {
            return Ok(base_path.join("zksync-era"));
        }
        let spinner = Spinner::new(MSG_CLONING_ERA_REPO_SPINNER);
        if !base_path.exists() {
            shell.create_dir(base_path)?;
        }
        let link_to_code = git::clone(shell, base_path, ZKSYNC_ERA_GIT_REPO, "zksync-era")?;
        spinner.finish();
        Ok(link_to_code)
    } else {
        let path = PathBuf::from_str(&link_to_code)?;
        if update_submodules.is_none() || update_submodules == Some(true) {
            git::submodule_update(shell, &path)?;
        }
        Ok(path)
    }
}
