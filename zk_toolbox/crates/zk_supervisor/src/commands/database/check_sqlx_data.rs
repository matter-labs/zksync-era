use std::path::Path;

use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use super::args::DatabaseCommonArgs;
use crate::dals::{get_dals, Dal};

pub fn run(shell: &Shell, args: DatabaseCommonArgs) -> anyhow::Result<()> {
    let args = args.parse();
    if args.selected_dals.none() {
        logger::outro("No databases selected to check");
        return Ok(());
    }

    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    logger::info("Checking sqlx data");

    let dals = get_dals(shell, &args.selected_dals)?;
    for dal in dals {
        check_sqlx_data(shell, &ecosystem_config.link_to_code, dal)?;
    }

    logger::outro("Databases sqlx data checked successfully");

    Ok(())
}

pub fn check_sqlx_data(
    shell: &Shell,
    link_to_code: impl AsRef<Path>,
    dal: Dal,
) -> anyhow::Result<()> {
    let dir = link_to_code.as_ref().join(&dal.path);
    let _dir_guard = shell.push_dir(dir);
    let url = dal.url.as_str();

    let spinner = Spinner::new(&format!("Checking sqlx data for dal {}...", dal.path));
    Cmd::new(cmd!(
        shell,
        "cargo sqlx prepare --check --database-url {url} -- --tests"
    ))
    .run()?;
    spinner.finish();

    Ok(())
}
