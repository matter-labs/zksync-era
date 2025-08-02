use anyhow::Context;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger};
use zkstack_cli_config::ZkStackConfig;

use crate::messages::{MSG_BUILDING_EN, MSG_FAILED_TO_BUILD_EN_ERR};

pub(crate) async fn build(shell: &Shell) -> anyhow::Result<()> {
    let link_to_code = ZkStackConfig::from_file(shell)?.link_to_code();
    let _dir_guard = shell.push_dir(link_to_code.join("core"));

    logger::info(MSG_BUILDING_EN);

    let mut cmd = Cmd::new(cmd!(
        shell,
        "cargo build --release --bin zksync_external_node"
    ));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_FAILED_TO_BUILD_EN_ERR)
}
