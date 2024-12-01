use anyhow::Context;
use common::{cmd::Cmd, logger};
use config::zkstack_config::ZkStackConfig;
use xshell::{cmd, Shell};

use crate::messages::{MSG_BUILDING_EN, MSG_FAILED_TO_BUILD_EN_ERR};

pub(crate) async fn build(shell: &Shell) -> anyhow::Result<()> {
    let chain = ZkStackConfig::current_chain(shell)?;
    let _dir_guard = shell.push_dir(&chain.link_to_code);

    logger::info(MSG_BUILDING_EN);

    let mut cmd = Cmd::new(cmd!(
        shell,
        "cargo build --release --bin zksync_external_node"
    ));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_FAILED_TO_BUILD_EN_ERR)
}
