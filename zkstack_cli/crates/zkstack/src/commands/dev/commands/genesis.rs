use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, db::DatabaseConfig, spinner::Spinner};
use zkstack_cli_config::{SecretsConfig, ZkStackConfig, SECRETS_FILE};

use crate::{
    commands::dev::{
        commands::database::reset::reset_database, dals::get_core_dal,
        messages::MSG_GENESIS_FILE_GENERATION_STARTED,
    },
    defaults::{generate_db_names, DBNames, DATABASE_SERVER_URL},
};

pub(crate) async fn run(shell: &Shell) -> anyhow::Result<()> {
    let chain = ZkStackConfig::current_chain(shell)?;
    let spinner = Spinner::new(MSG_GENESIS_FILE_GENERATION_STARTED);
    let tmp_dir = shell.create_temp_dir()?;
    let secrets_path = match chain.path_to_secrets_config().canonicalize() {
        Ok(path) => path,
        Err(_) => {
            // If secrets file does not exist, create a temporary one.
            let tmp_path = tmp_dir.path().to_path_buf();
            let _data = shell.push_dir(tmp_path.clone());
            shell.write_file(SECRETS_FILE, "db: \n")?;
            let mut secrets = SecretsConfig::read(shell, &tmp_path.join(SECRETS_FILE))
                .await?
                .patched();
            let DBNames { server_name, .. } = generate_db_names(&chain);
            let db_config = DatabaseConfig::new(DATABASE_SERVER_URL.clone(), server_name);
            secrets.set_server_database(&db_config)?;
            secrets.save().await?;
            tmp_path.join(SECRETS_FILE)
        }
    };
    let secrets = SecretsConfig::read(shell, &secrets_path).await?;
    let dal = get_core_dal(
        shell,
        Some(secrets.core_database_url()?.unwrap().to_string()),
    )
    .await?;
    reset_database(shell, chain.link_to_code.clone(), dal).await?;
    shell.change_dir(chain.link_to_code);
    let _dir = shell.push_dir("core");
    Cmd::new(cmd!(shell,"cargo run --package genesis_generator --bin genesis_generator -- --config-path={secrets_path}")).run()?;
    spinner.finish();
    Ok(())
}
