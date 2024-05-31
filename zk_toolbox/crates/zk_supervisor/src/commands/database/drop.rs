use common::{
    db::{drop_db_if_exists, DatabaseConfig},
    logger,
    spinner::Spinner,
};
use xshell::Shell;

use super::args::DatabaseCommonArgs;
use crate::dals::{get_dals, Dal};

pub async fn run(shell: &Shell, args: DatabaseCommonArgs) -> anyhow::Result<()> {
    let args = args.parse();
    if args.selected_dals.none() {
        logger::outro("No databases selected to drop");
        return Ok(());
    }

    logger::info("Dropping databases");

    let dals = get_dals(shell, &args.selected_dals)?;
    for dal in dals {
        drop_database(dal).await?;
    }

    logger::outro("Databases dropped successfully");

    Ok(())
}

pub async fn drop_database(dal: Dal) -> anyhow::Result<()> {
    let spinner = Spinner::new(&format!("Dropping DB for dal {}...", dal.path));
    let db = DatabaseConfig::from_url(dal.url)?;
    drop_db_if_exists(&db).await?;
    spinner.finish();
    Ok(())
}
