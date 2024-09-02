use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use strum::EnumIter;
use xshell::{cmd, Shell};

use crate::messages::{
    MSG_BUILDING_CONTRACTS, MSG_BUILDING_CONTRACTS_SUCCESS, MSG_BUILDING_L1_CONTRACTS_SPINNER,
    MSG_BUILDING_L2_CONTRACTS_SPINNER, MSG_BUILDING_SYSTEM_CONTRACTS_SPINNER,
    MSG_CONTRACTS_DEPS_SPINNER,
};

#[derive(Debug, Parser)]
pub struct ContractsArgs {
    #[clap(long, short = 'c')]
    pub contracts: Vec<ContractType>,
}

#[derive(Debug, ValueEnum, EnumIter, strum::Display, PartialEq, Eq, Clone, Copy)]
#[strum(serialize_all = "lowercase")]
pub enum ContractType {
    L1,
    L2,
    SystemContracts,
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
    logger::info(MSG_BUILDING_CONTRACTS);

    let ecosystem = EcosystemConfig::from_file(shell)?;
    let link_to_code = ecosystem.link_to_code.clone();

    let contracts = if args.contracts.is_empty() {
        vec![
            ContractType::L1,
            ContractType::L2,
            ContractType::SystemContracts,
        ]
    } else {
        args.contracts.clone()
    };

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
