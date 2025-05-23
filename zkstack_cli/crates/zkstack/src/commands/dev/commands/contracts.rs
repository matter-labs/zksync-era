use std::path::PathBuf;

use clap::Parser;
use xshell::Shell;
use zkstack_cli_common::{
    contracts::{
        build_l1_contracts, build_l1_da_contracts, build_l2_contracts, build_system_contracts,
        build_tee_contracts,
    },
    logger,
    spinner::Spinner,
};
use zkstack_cli_config::EcosystemConfig;

use crate::commands::dev::messages::{
    MSG_BUILDING_CONTRACTS, MSG_BUILDING_CONTRACTS_SUCCESS, MSG_BUILDING_L1_CONTRACTS_SPINNER,
    MSG_BUILDING_L1_DA_CONTRACTS_SPINNER, MSG_BUILDING_L2_CONTRACTS_SPINNER,
    MSG_BUILDING_SYSTEM_CONTRACTS_SPINNER, MSG_BUILDING_TEE_CONTRACTS_SPINNER,
    MSG_BUILD_L1_CONTRACTS_HELP, MSG_BUILD_L1_DA_CONTRACTS_HELP, MSG_BUILD_L2_CONTRACTS_HELP,
    MSG_BUILD_SYSTEM_CONTRACTS_HELP, MSG_BUILD_TEE_CONTRACTS_HELP, MSG_NOTHING_TO_BUILD_MSG,
};

#[derive(Debug, Parser)]
pub struct ContractsArgs {
    #[clap(long, alias = "l1", help = MSG_BUILD_L1_CONTRACTS_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub l1_contracts: Option<bool>,
    #[clap(long, alias = "l1-da", help = MSG_BUILD_L1_DA_CONTRACTS_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub l1_da_contracts: Option<bool>,
    #[clap(long, alias = "l2", help = MSG_BUILD_L2_CONTRACTS_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub l2_contracts: Option<bool>,
    #[clap(long, alias = "sc", help = MSG_BUILD_SYSTEM_CONTRACTS_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub system_contracts: Option<bool>,
    #[clap(long, alias = "tee", help = MSG_BUILD_TEE_CONTRACTS_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub tee_contracts: Option<bool>,
}

impl ContractsArgs {
    fn contracts(&self) -> Vec<ContractType> {
        if self.l1_contracts.is_none()
            && self.l2_contracts.is_none()
            && self.system_contracts.is_none()
            && self.tee_contracts.is_none()
            && self.l1_da_contracts.is_none()
        {
            return vec![
                ContractType::L1,
                ContractType::L1DA,
                ContractType::L2,
                ContractType::SystemContracts,
                ContractType::Tee,
            ];
        }

        let mut contracts = vec![];
        if self.l1_contracts.unwrap_or(false) {
            contracts.push(ContractType::L1);
        }
        if self.l1_da_contracts.unwrap_or(false) {
            contracts.push(ContractType::L1DA);
        }
        if self.l2_contracts.unwrap_or(false) {
            contracts.push(ContractType::L2);
        }
        if self.system_contracts.unwrap_or(false) {
            contracts.push(ContractType::SystemContracts);
        }
        if self.tee_contracts.unwrap_or(false) {
            contracts.push(ContractType::Tee);
        }
        contracts
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ContractType {
    L1,
    L1DA,
    L2,
    SystemContracts,
    Tee,
}

struct ContractBuilder {
    cmd: Box<dyn FnOnce(Shell, PathBuf) -> anyhow::Result<()>>,
    msg: String,
    link_to_code: PathBuf,
}

impl ContractBuilder {
    fn new(ecosystem: &EcosystemConfig, contract_type: ContractType) -> Self {
        match contract_type {
            ContractType::L1 => Self {
                cmd: Box::new(build_l1_contracts),
                msg: MSG_BUILDING_L1_CONTRACTS_SPINNER.to_string(),
                link_to_code: ecosystem.link_to_code.clone(),
            },
            ContractType::L1DA => Self {
                cmd: Box::new(build_l1_da_contracts),
                msg: MSG_BUILDING_L1_DA_CONTRACTS_SPINNER.to_string(),
                link_to_code: ecosystem.link_to_code.clone(),
            },
            ContractType::L2 => Self {
                cmd: Box::new(build_l2_contracts),
                msg: MSG_BUILDING_L2_CONTRACTS_SPINNER.to_string(),
                link_to_code: ecosystem.link_to_code.clone(),
            },
            ContractType::SystemContracts => Self {
                cmd: Box::new(build_system_contracts),
                msg: MSG_BUILDING_SYSTEM_CONTRACTS_SPINNER.to_string(),
                link_to_code: ecosystem.link_to_code.clone(),
            },
            ContractType::Tee => Self {
                cmd: Box::new(build_tee_contracts),
                msg: MSG_BUILDING_TEE_CONTRACTS_SPINNER.to_string(),
                link_to_code: ecosystem.link_to_code.clone(),
            },
        }
    }

    fn build(self, shell: Shell) -> anyhow::Result<()> {
        let spinner = Spinner::new(&self.msg);
        (self.cmd)(shell, self.link_to_code.clone())?;
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

    contracts
        .iter()
        .map(|contract| ContractBuilder::new(&ecosystem, *contract))
        .try_for_each(|builder| builder.build(shell.clone()))?;

    logger::outro(MSG_BUILDING_CONTRACTS_SUCCESS);

    Ok(())
}
