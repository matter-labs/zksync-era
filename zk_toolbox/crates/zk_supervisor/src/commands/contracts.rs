use std::path::PathBuf;

use clap::Parser;
use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::messages::{
    MSG_BUILDING_CONTRACTS, MSG_BUILDING_CONTRACTS_SUCCESS, MSG_BUILDING_L1_CONTRACTS_SPINNER,
    MSG_BUILDING_L2_CONTRACTS_SPINNER, MSG_BUILDING_SYSTEM_CONTRACTS_SPINNER,
    MSG_BUILDING_TEST_CONTRACTS_SPINNER, MSG_BUILD_L1_CONTRACTS_HELP, MSG_BUILD_L2_CONTRACTS_HELP,
    MSG_BUILD_SYSTEM_CONTRACTS_HELP, MSG_BUILD_TEST_CONTRACTS_HELP, MSG_CONTRACTS_DEPS_SPINNER,
    MSG_NOTHING_TO_BUILD_MSG,
};

#[derive(Debug, Parser)]
pub struct ContractsArgs {
    #[clap(long, alias = "l1", help = MSG_BUILD_L1_CONTRACTS_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub l1_contracts: Option<bool>,
    #[clap(long, alias = "l2", help = MSG_BUILD_L2_CONTRACTS_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub l2_contracts: Option<bool>,
    #[clap(long, alias = "sc", help = MSG_BUILD_SYSTEM_CONTRACTS_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub system_contracts: Option<bool>,
    #[clap(long, alias = "test", help = MSG_BUILD_TEST_CONTRACTS_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub test_contracts: Option<bool>,
}

impl ContractsArgs {
    fn contracts(&self) -> Vec<ContractType> {
        if self.l1_contracts.is_none()
            && self.l2_contracts.is_none()
            && self.system_contracts.is_none()
            && self.test_contracts.is_none()
        {
            return vec![
                ContractType::L1,
                ContractType::L2,
                ContractType::SystemContracts,
                ContractType::TestContracts,
            ];
        }

        let mut contracts = vec![];

        if self.l1_contracts.unwrap_or(false) {
            contracts.push(ContractType::L1);
        }
        if self.l2_contracts.unwrap_or(false) {
            contracts.push(ContractType::L2);
        }
        if self.system_contracts.unwrap_or(false) {
            contracts.push(ContractType::SystemContracts);
        }
        if self.test_contracts.unwrap_or(false) {
            contracts.push(ContractType::TestContracts);
        }

        contracts
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ContractType {
    L1,
    L2,
    SystemContracts,
    TestContracts,
}

#[derive(Debug)]
struct ContractBuilder {
    dir: PathBuf,
    cmd: String,
    msg: String,
}

impl ContractBuilder {
    fn new(ecosystem: &EcosystemConfig, contract_type: ContractType) -> Self {
        match contract_type {
            ContractType::L1 => Self {
                dir: ecosystem.path_to_foundry(),
                cmd: "forge build".to_string(),
                msg: MSG_BUILDING_L1_CONTRACTS_SPINNER.to_string(),
            },
            ContractType::L2 => Self {
                dir: ecosystem.link_to_code.clone(),
                cmd: "yarn l2-contracts build".to_string(),
                msg: MSG_BUILDING_L2_CONTRACTS_SPINNER.to_string(),
            },
            ContractType::SystemContracts => Self {
                dir: ecosystem.link_to_code.join("contracts"),
                cmd: "yarn sc build".to_string(),
                msg: MSG_BUILDING_SYSTEM_CONTRACTS_SPINNER.to_string(),
            },
            ContractType::TestContracts => Self {
                dir: ecosystem.link_to_code.join("etc/contracts-test-data"),
                cmd: "yarn build".to_string(),
                msg: MSG_BUILDING_TEST_CONTRACTS_SPINNER.to_string(),
            },
        }
    }

    fn build(&self, shell: &Shell) -> anyhow::Result<()> {
        let spinner = Spinner::new(&self.msg);
        let _dir_guard = shell.push_dir(&self.dir);

        let mut args = self.cmd.split_whitespace().collect::<Vec<_>>();
        let command = args.remove(0); // It's safe to unwrap here because we know that the vec is not empty
        let mut cmd = cmd!(shell, "{command}");

        for arg in args {
            cmd = cmd.arg(arg);
        }

        Cmd::new(cmd).run()?;

        spinner.finish();
        Ok(())
    }
}

pub fn run(shell: &Shell, args: ContractsArgs) -> anyhow::Result<()> {
    let contracts = args.contracts();
    if contracts.is_empty() {
        logger::outro(MSG_NOTHING_TO_BUILD_MSG);
        return Ok(());
    }

    logger::info(MSG_BUILDING_CONTRACTS);

    let ecosystem = EcosystemConfig::from_file(shell)?;
    let link_to_code = ecosystem.link_to_code.clone();

    let spinner = Spinner::new(MSG_CONTRACTS_DEPS_SPINNER);
    let _dir_guard = shell.push_dir(&link_to_code);
    Cmd::new(cmd!(shell, "yarn install")).run()?;
    spinner.finish();

    contracts
        .iter()
        .map(|contract| ContractBuilder::new(&ecosystem, *contract))
        .try_for_each(|builder| builder.build(shell))?;

    logger::outro(MSG_BUILDING_CONTRACTS_SUCCESS);

    Ok(())
}
