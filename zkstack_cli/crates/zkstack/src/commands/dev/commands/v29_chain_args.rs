use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use url::Url;
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, Prompt};
use zkstack_cli_config::EcosystemConfig;
use zkstack_cli_types::L1Network;

use crate::{
    defaults::LOCAL_RPC_URL,
    messages::{
        MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT,
        MSG_SERVER_COMMAND_HELP,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct V29ChainUpgradeArgs {
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
}

impl V29ChainUpgradeArgs {
    pub fn fill_if_empyty(mut self, shell: &Shell) -> anyhow::Result<Self> {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        self.chain_id = Some(
            self.chain_id
                .unwrap_or(ecosystem_config.era_chain_id.as_u64()),
        );
        self.l1_rpc_url = Some(
            self.l1_rpc_url
                .unwrap_or("http://localhost:8545".to_string()),
        );

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
        self.gw_chain_id = Some(self.gw_chain_id.unwrap_or(506));
        self.l1_gas_price = Some(self.l1_gas_price.unwrap_or(100000));
        self.l2_rpc_url = Some(
            self.l2_rpc_url
                .unwrap_or("http://localhost:3050".to_string()),
        );
        Ok(self)
    }
}

pub struct V29UpgradeArgsInner {
    pub chain_id: u64,
    pub l1_rpc_url: String,
    pub gw_rpc_url: String,
}

impl From<V29ChainUpgradeArgs> for V29UpgradeArgsInner {
    fn from(value: V29ChainUpgradeArgs) -> Self {
        Self {
            chain_id: value.chain_id.unwrap(),
            l1_rpc_url: value.l1_rpc_url.unwrap(),
            gw_rpc_url: value.gw_rpc_url.unwrap(),
        }
    }
}
