use args::build_transactions::BuildTransactionsArgs;
use clap::Subcommand;
use xshell::Shell;
use zkstack_cli_common::{git, spinner::Spinner};

use crate::{
    commands::ecosystem::args::{
        change_default::ChangeDefaultChain,
        create::EcosystemCreateArgs,
        init::{EcosystemInitArgs, InitCoreContractsArgs},
        register_new_ctm::RegisterCTMArgs,
    },
    messages::{MSG_BUILDING_CONTRACTS, MSG_UPDATING_SUBMODULES_SPINNER},
};

pub mod args;
pub(crate) mod build_transactions;
mod change_default;
mod common;
mod create;
pub mod create_configs;
pub(crate) mod init;
pub(crate) mod init_core_contracts;
pub(crate) mod register_ctm;
pub(crate) mod setup_observability;

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum EcosystemCommands {
    /// Create a new ecosystem and chain,
    /// setting necessary configurations for later initialization
    Create(EcosystemCreateArgs),
    /// Create transactions to build ecosystem contracts
    BuildTransactions(BuildTransactionsArgs),
    /// Initialize ecosystem and chain,
    /// deploying necessary contracts and performing on-chain operations
    Init(EcosystemInitArgs),
    /// Initialize ecosystem core contracts
    InitCoreContracts(InitCoreContractsArgs),
    /// Change the default chain
    #[command(alias = "cd")]
    ChangeDefaultChain(ChangeDefaultChain),
    /// Setup observability for the ecosystem,
    /// downloading Grafana dashboards from the era-observability repo
    #[command(alias = "obs")]
    SetupObservability,
    /// Register a new CTM on an existing BridgeHub
    RegisterCTM(RegisterCTMArgs),
}

pub(crate) async fn run(shell: &Shell, args: EcosystemCommands) -> anyhow::Result<()> {
    let ecosystem = zkstack_cli_config::ZkStackConfig::ecosystem(shell);
    if let Ok(ecosystem) = &ecosystem {
        if args.update_submodules() {
            let spinner = Spinner::new(MSG_UPDATING_SUBMODULES_SPINNER);
            git::submodule_update(shell, &ecosystem.link_to_code())?;
            spinner.finish();
        }
        if args.rebuild_contracts() {
            let spinner = Spinner::new(MSG_BUILDING_CONTRACTS);
            zkstack_cli_common::contracts::rebuild_all_contracts(
                shell,
                &ecosystem.contracts_path_for_ctm(args.zksync_os()),
            )?;
            spinner.finish();
        }
    }

    match args {
        EcosystemCommands::Create(args) => create::run(args, shell).await,
        EcosystemCommands::BuildTransactions(args) => build_transactions::run(args, shell).await,
        EcosystemCommands::Init(args) => init::run(args, shell).await,
        EcosystemCommands::InitCoreContracts(args) => init_core_contracts::run(args, shell).await,
        EcosystemCommands::ChangeDefaultChain(args) => change_default::run(args, shell),
        EcosystemCommands::SetupObservability => setup_observability::run(shell),
        EcosystemCommands::RegisterCTM(args) => register_ctm::run(args, shell).await,
    }
}

impl EcosystemCommands {
    fn update_submodules(&self) -> bool {
        match self {
            EcosystemCommands::Create(_) => false,
            EcosystemCommands::ChangeDefaultChain(_) => false,
            EcosystemCommands::SetupObservability => false,
            EcosystemCommands::BuildTransactions(args) => args.common.update_submodules,
            EcosystemCommands::Init(args) => args.common.update_submodules,
            EcosystemCommands::InitCoreContracts(args) => args.common.update_submodules,
            EcosystemCommands::RegisterCTM(args) => args.common.update_submodules,
        }
    }

    fn rebuild_contracts(&self) -> bool {
        match self {
            EcosystemCommands::Create(_) => false,
            EcosystemCommands::ChangeDefaultChain(_) => false,
            EcosystemCommands::SetupObservability => false,
            EcosystemCommands::BuildTransactions(args) => {
                !args.common.skip_contract_compilation_override
            }
            EcosystemCommands::Init(args) => !args.common.skip_contract_compilation_override,
            EcosystemCommands::InitCoreContracts(args) => {
                !args.common.skip_contract_compilation_override
            }
            EcosystemCommands::RegisterCTM(args) => !args.common.skip_contract_compilation_override,
        }
    }

    fn zksync_os(&self) -> bool {
        match self {
            EcosystemCommands::Create(_) => false,
            EcosystemCommands::ChangeDefaultChain(_) => false,
            EcosystemCommands::SetupObservability => false,
            EcosystemCommands::BuildTransactions(args) => args.common.zksync_os,
            EcosystemCommands::Init(args) => args.common.zksync_os,
            EcosystemCommands::InitCoreContracts(args) => args.common.zksync_os,
            EcosystemCommands::RegisterCTM(args) => args.common.zksync_os,
        }
    }
}
