use anyhow::Context;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, spinner::Spinner};
use zkstack_cli_config::EcosystemConfig;

use crate::{
    commands::dev::{
        commands::database::reset::reset_database, dals::get_core_dal,
        messages::MSG_GENESIS_FILE_GENERATION_STARTED,
    },
    messages::MSG_CHAIN_NOT_FOUND_ERR,
};

pub(crate) async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem
        .load_chain(Some(ecosystem.current_chain().to_string()))
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let spinner = Spinner::new(MSG_GENESIS_FILE_GENERATION_STARTED);
    let secrets_path = chain.path_to_secrets_config().canonicalize().unwrap();
    let dal = get_core_dal(shell, None).await?;
    reset_database(shell, ecosystem.link_to_code.clone(), dal).await?;
    let core_path = ecosystem.link_to_code.clone().join("core");
    let _dir = shell.push_dir(&core_path);
    Cmd::new(cmd!(shell,"cargo run --package genesis_generator --bin genesis_generator -- --config-path={secrets_path}")).run()?;
    spinner.finish();
    Ok(())
}
