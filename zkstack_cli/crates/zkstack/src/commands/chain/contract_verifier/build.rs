use anyhow::Context;
use common::{cmd::Cmd, logger};
use config::ChainConfig;
use xshell::{cmd, Shell};

use crate::messages::{MSG_BUILDING_CONTRACT_VERIFIER, MSG_FAILED_TO_BUILD_CONTRACT_VERIFIER_ERR};

pub(crate) async fn build(shell: &Shell, chain: ChainConfig) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(&chain.link_to_code);

    logger::info(MSG_BUILDING_CONTRACT_VERIFIER);

    let mut cmd = Cmd::new(cmd!(
        shell,
        "cargo build --release --bin zksync_contract_verifier"
    ));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_FAILED_TO_BUILD_CONTRACT_VERIFIER_ERR)
}
