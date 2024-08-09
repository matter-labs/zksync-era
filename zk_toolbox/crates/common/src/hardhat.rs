use std::path::Path;

use xshell::{cmd, Shell};

use crate::cmd::Cmd;

pub fn build_l2_contracts(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts"));
    Ok(Cmd::new(cmd!(shell, "yarn l2 build")).run()?)
}
