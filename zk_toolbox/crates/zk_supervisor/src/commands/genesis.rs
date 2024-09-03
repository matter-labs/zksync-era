use anyhow::Context;
use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::{
    commands::database::reset::reset_database,
    dals::get_core_dal,
    messages::{MSG_CHAIN_NOT_FOUND_ERR, MSG_GENESIS_FILE_GENERATION_STARTED, MSG_GENESIS_SUCCESS},
};

pub(crate) async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem
        .load_chain(Some(ecosystem.current_chain().to_string()))
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let spinner = Spinner::new(MSG_GENESIS_FILE_GENERATION_STARTED);
    let secrets_path = chain.path_to_secrets_config();
    let dal = get_core_dal(shell, None)?;
    reset_database(shell, ecosystem.link_to_code, dal).await?;
    Cmd::new(cmd!(shell,"cargo run --package genesis_generator --bin genesis_generator -- --config-path={secrets_path}")).run()?;
    spinner.finish();
    Ok(())
}
