use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use zkstack_cli_common::forge::ForgeScriptArgs;

use crate::{
    commands::{
        dev::commands::upgrades::types::UpgradeVersion,
        ecosystem::args::common::CommonEcosystemArgs,
    },
    messages::{MSG_L1_RPC_URL_HELP, MSG_SERVER_COMMAND_HELP},
};

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, ValueEnum, EnumIter, strum::Display, PartialEq, Eq,
)]
pub enum EcosystemUpgradeStage {
    // Deploy contracts + init everything the governance will need to approve the upgrade
    NoGovernancePrepare,
    // Ecosystem admin will execute its calls (typically only server notifier upgrade)
    EcosystemAdmin,
    /// Pause migration to/from Gateway
    GovernanceStage0,
    // Governance will execute stage 1 of the upgrade, which appends
    // a new protocol version and all chains involved must upgrade
    GovernanceStage1,
    // Governance will execute stage 2 of the upgrade. It is CRUCIAL
    // to have it done only after protocol deadline has passed.
    // Unpause migrations, etc.
    GovernanceStage2,
    // Finish finalizing tokens, chains, etc
    NoGovernanceStage2,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemUpgradeArgs {
    #[clap(flatten)]
    pub common: CommonEcosystemArgs,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long, value_enum)]
    pub upgrade_version: UpgradeVersion,
    #[clap(long, value_enum)]
    ecosystem_upgrade_stage: EcosystemUpgradeStage,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
    #[clap(long, help = MSG_SERVER_COMMAND_HELP)]
    pub server_command: Option<String>,
}

impl EcosystemUpgradeArgs {
    #[allow(dead_code)]
    pub fn fill_values_with_prompt(self, run_upgrade: bool) -> EcosystemUpgradeArgsFinal {
        EcosystemUpgradeArgsFinal {
            forge_args: self.forge_args,
            ecosystem_upgrade_stage: self.ecosystem_upgrade_stage,
            l1_rpc_url: self.l1_rpc_url,
            server_command: self.server_command,
            run_upgrade,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemUpgradeArgsFinal {
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long, value_enum)]
    pub ecosystem_upgrade_stage: EcosystemUpgradeStage,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
    #[clap(long, help = MSG_SERVER_COMMAND_HELP)]
    pub server_command: Option<String>,
    pub run_upgrade: bool,
}
