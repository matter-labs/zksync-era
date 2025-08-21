use anyhow::Context;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger};
use zkstack_cli_config::ZkStackConfig;

use crate::messages::{
    MSG_BUILDING_CONTRACT_VERIFIER, MSG_CHAIN_NOT_FOUND_ERR,
    MSG_FAILED_TO_BUILD_CONTRACT_VERIFIER_ERR,
};

pub(crate) async fn build(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem = ZkStackConfig::ecosystem(shell)?;
    let chain = ecosystem
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let _dir_guard = shell.push_dir(chain.link_to_code.join("core"));

    logger::info(MSG_BUILDING_CONTRACT_VERIFIER);

    let mut cmd = Cmd::new(cmd!(
        shell,
        "cargo build --release --bin zksync_contract_verifier"
    ));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_FAILED_TO_BUILD_CONTRACT_VERIFIER_ERR)
}
