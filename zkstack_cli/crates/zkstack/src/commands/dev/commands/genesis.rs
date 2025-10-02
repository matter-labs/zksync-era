use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, spinner::Spinner};
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};

use crate::commands::dev::{
    commands::database::reset::reset_database, dals::get_core_dal,
    messages::MSG_GENESIS_FILE_GENERATION_STARTED,
};

pub(crate) async fn run(shell: &Shell) -> anyhow::Result<()> {
    let chain = ZkStackConfig::current_chain(shell)?;
    let spinner = Spinner::new(MSG_GENESIS_FILE_GENERATION_STARTED);
    let secrets_path = chain.path_to_secrets_config().canonicalize().unwrap();
    let dal = get_core_dal(shell, None).await?;
    reset_database(shell, chain.link_to_code(), dal).await?;
    let _dir = shell.push_dir("core");
    Cmd::new(cmd!(shell,"cargo run --package genesis_generator --bin genesis_generator -- --config-path={secrets_path}")).run()?;
    spinner.finish();
    Ok(())
}
