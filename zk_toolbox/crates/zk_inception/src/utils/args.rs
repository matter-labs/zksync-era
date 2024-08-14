use std::path::Path;

use anyhow::bail;
use common::{cmd::Cmd, logger, Prompt, PromptConfirm};
use strum::EnumIter;
use xshell::{cmd, Shell};

use crate::messages::{
    msg_path_to_zksync_does_not_exist_err, MSG_CONFIRM_STILL_USE_FOLDER, MSG_LINK_TO_CODE_PROMPT,
    MSG_NOT_MAIN_REPO_OR_FORK_ERR,
};

#[derive(Debug, Clone, EnumIter, PartialEq, Eq)]
pub(crate) enum LinkToCodeSelection {
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

pub(crate) fn check_link_to_code(shell: &Shell, path: &str) -> anyhow::Result<()> {
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
pub(crate) fn pick_new_link_to_code(shell: &Shell) -> String {
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
