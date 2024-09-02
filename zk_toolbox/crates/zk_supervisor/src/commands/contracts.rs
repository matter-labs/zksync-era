use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::messages::{
    MSG_BUILDING_CONTRACTS, MSG_BUILDING_CONTRACTS_SUCCESS, MSG_BUILDING_L1_CONTRACTS_SPINNER,
    MSG_BUILDING_L2_CONTRACTS_SPINNER, MSG_BUILDING_SYSTEM_CONTRACTS_SPINNER,
    MSG_CONTRACTS_DEPS_SPINNER,
};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_BUILDING_CONTRACTS);

    let ecosystem = EcosystemConfig::from_file(shell)?;
    let link_to_code = ecosystem.link_to_code.clone();

    let spinner = Spinner::new(MSG_CONTRACTS_DEPS_SPINNER);
    let _dir_guard = shell.push_dir(&link_to_code);
    Cmd::new(cmd!(shell, "yarn install")).run()?;
    spinner.finish();

    let spinner = Spinner::new(MSG_BUILDING_L1_CONTRACTS_SPINNER);
    let foundry_contracts_path = ecosystem.path_to_foundry();
    let _dir_guard = shell.push_dir(&foundry_contracts_path);
    Cmd::new(cmd!(shell, "forge build")).run()?;
    spinner.finish();

    let spinner = Spinner::new(MSG_BUILDING_SYSTEM_CONTRACTS_SPINNER);
    let _dir_guard = shell.push_dir(link_to_code.join("contracts"));
    Cmd::new(cmd!(shell, "yarn sc build")).run()?;
    spinner.finish();

    let spinner = Spinner::new(MSG_BUILDING_L2_CONTRACTS_SPINNER);
    let _dir_guard = shell.push_dir(&link_to_code);
    Cmd::new(cmd!(shell, "yarn l2-contracts build")).run()?;
    spinner.finish();

    logger::outro(MSG_BUILDING_CONTRACTS_SUCCESS);

    Ok(())
}
