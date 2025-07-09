use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use url::Url;
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, Prompt};
use zkstack_cli_config::EcosystemConfig;
use zkstack_cli_types::L1Network;

use crate::commands::dev::commands::v29_ecosystem_args::UpgradeVersions;
use crate::{
    defaults::LOCAL_RPC_URL,
    messages::{
        MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT,
        MSG_SERVER_COMMAND_HELP,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct ChainUpgradeArgs {
    pub upgrade_description_path: Option<String>,
    pub chain_id: Option<u64>,
    pub gw_chain_id: Option<u64>,
    pub l1_gas_price: Option<u64>,
    pub l1_rpc_url: Option<String>,
    pub l2_rpc_url: Option<String>,
    pub gw_rpc_url: Option<String>,
    pub server_upgrade_timestamp: Option<u64>,
    #[clap(long, default_missing_value = "false")]
    pub dangerous_no_cross_check: Option<bool>,
    #[clap(long, default_missing_value = "false")]
    pub force_display_finalization_params: Option<bool>,
    #[clap(long, value_enum)]
    pub upgrade_version: UpgradeVersions,
}

impl ChainUpgradeArgs {
    pub async fn fill_if_empty(mut self, shell: &Shell) -> anyhow::Result<Self> {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        let chain_config = ecosystem_config.load_current_chain()?;
        self.chain_id = Some(self.chain_id.unwrap_or(chain_config.chain_id.as_u64()));

        self.server_upgrade_timestamp = Some(
            self.server_upgrade_timestamp.unwrap_or(
                std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
        );
        self.gw_rpc_url = Some(
            self.gw_rpc_url
                .unwrap_or("http://localhost:3250".to_string()),
        );

        self.gw_rpc_url = if let Some(url) = self.gw_rpc_url {
            Some(url)
        } else {
            chain_config.get_secrets_config().await?.gw_rpc_url().ok()
        };

        self.l1_rpc_url = if let Some(url) = self.l1_rpc_url {
            Some(url)
        } else {
            chain_config.get_secrets_config().await?.l1_rpc_url().ok()
        };

        self.gw_chain_id = Some(self.gw_chain_id.unwrap_or(506));
        self.l1_gas_price = Some(self.l1_gas_price.unwrap_or(100000));
        self.l2_rpc_url = if let Some(url) = self.l2_rpc_url {
            Some(url)
        } else {
            chain_config.get_general_config().await?.l2_http_url().ok()
        };

        Ok(self)
    }
}

pub struct UpgradeArgsInner {
    pub chain_id: u64,
    pub l1_rpc_url: String,
    pub gw_rpc_url: String,
}

impl From<ChainUpgradeArgs> for UpgradeArgsInner {
    fn from(value: ChainUpgradeArgs) -> Self {
        Self {
            chain_id: value.chain_id.unwrap(),
            l1_rpc_url: value.l1_rpc_url.unwrap(),
            gw_rpc_url: value.gw_rpc_url.unwrap(),
        }
    }
}
