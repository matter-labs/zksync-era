use std::path::Path;

use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use super::args::new_migration::{DatabaseNewMigrationArgs, SelectedDatabase};
use crate::{
    dals::{get_core_dal, get_prover_dal, Dal},
    messages::{msg_database_new_migration_loading, MSG_DATABASE_NEW_MIGRATION_SUCCESS},
};

pub fn run(shell: &Shell, args: DatabaseNewMigrationArgs) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();

    let dal = match args.selected_database {
        SelectedDatabase::Core => get_core_dal(shell)?,
        SelectedDatabase::Prover => get_prover_dal(shell)?,
    };
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    generate_migration(shell, ecosystem_config.link_to_code, dal, args.name)?;

    logger::outro(MSG_DATABASE_NEW_MIGRATION_SUCCESS);

    Ok(())
}

fn generate_migration(
    shell: &Shell,
    link_to_code: impl AsRef<Path>,
    dal: Dal,
    name: String,
) -> anyhow::Result<()> {
    let dir = link_to_code.as_ref().join(&dal.path);
    let _dir_guard = shell.push_dir(dir);

    let spinner = Spinner::new(&msg_database_new_migration_loading(&dal.path));
    Cmd::new(cmd!(shell, "cargo sqlx migrate add -r {name}")).run()?;
    spinner.finish();

    Ok(())
}
