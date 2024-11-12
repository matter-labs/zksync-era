use anyhow::Context;
use common::{cmd::Cmd, logger};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::messages::{MSG_BUILDING_EN, MSG_CHAIN_NOT_FOUND_ERR, MSG_FAILED_TO_BUILD_EN_ERR};

pub(crate) async fn build(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let _dir_guard = shell.push_dir(&chain.link_to_code);

    logger::info(MSG_BUILDING_EN);

    let mut cmd = Cmd::new(cmd!(
        shell,
        "cargo build --release --bin zksync_external_node"
    ));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_FAILED_TO_BUILD_EN_ERR)
}
