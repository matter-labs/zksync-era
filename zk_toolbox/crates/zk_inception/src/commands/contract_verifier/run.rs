use anyhow::Context;
use common::{cmd::Cmd, logger};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::messages::{
    MSG_CHAIN_NOT_FOUND_ERR, MSG_FAILED_TO_RUN_CONTRACT_VERIFIER_ERR, MSG_RUNNING_CONTRACT_VERIFIER,
};

pub(crate) async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem
        .load_chain(Some(ecosystem.default_chain.clone()))
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();

    let _dir_guard = shell.push_dir(&chain.link_to_code);

    logger::info(MSG_RUNNING_CONTRACT_VERIFIER);

    Cmd::new(cmd!(
        shell,
        "cargo run --bin zksync_contract_verifier -- --config-path={config_path} --secrets-path={secrets_path}"
    )).run().context(MSG_FAILED_TO_RUN_CONTRACT_VERIFIER_ERR)
}
