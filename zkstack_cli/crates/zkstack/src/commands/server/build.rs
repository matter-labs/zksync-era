use anyhow::Context;
use common::{cmd::Cmd, logger};
use config::ChainConfig;
use xshell::{cmd, Shell};

use crate::messages::{MSG_BUILDING_SERVER, MSG_FAILED_TO_BUILD_SERVER_ERR};

pub(super) fn build_server(chain_config: &ChainConfig, shell: &Shell) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(&chain_config.link_to_code);

    logger::info(MSG_BUILDING_SERVER);

    let mut cmd = Cmd::new(cmd!(shell, "cargo build --release --bin zksync_server"));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_FAILED_TO_BUILD_SERVER_ERR)
}
