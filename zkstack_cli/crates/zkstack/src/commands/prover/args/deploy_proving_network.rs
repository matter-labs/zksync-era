use clap::Parser;
use serde::{Deserialize, Serialize};
use zkstack_cli_common::{Prompt, PromptConfirm};

use crate::messages::{
    MSG_FERMAH_ADDRESS_PROMPT, MSG_L1_RPC_URL_PROMPT, MSG_LAGRANGE_ADDRESS_PROMPT,
    MSG_PROOF_MANAGER_OWNER_ADDRESS_PROMPT, MSG_PROXY_OWNER_ADDRESS_PROMPT, MSG_RPC_URL_PROMPT,
    MSG_TOP_UP_NETWORK_WALLETS_PROMPT, MSG_TOP_UP_SERVER_WALLETS_PROMPT, MSG_USDC_ADDRESS_PROMPT,
};

#[derive(Debug, Clone, Parser, Default, Serialize, Deserialize)]
pub struct DeployProvingNetworkArgs {
    #[clap(long)]
    pub rpc_url: Option<String>,
    #[clap(long)]
    pub l1_rpc_url: Option<String>,
    #[clap(long)]
    pub fermah_address: Option<String>,
    #[clap(long)]
    pub lagrange_address: Option<String>,
    #[clap(long)]
    pub usdc_address: Option<String>,
    #[clap(long)]
    pub proof_manager_owner_address: Option<String>,
    #[clap(long)]
    pub proxy_owner_address: Option<String>,
    #[clap(long)]
    pub top_up_server_wallets: Option<bool>,
    #[clap(long)]
    pub top_up_network_wallets: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployProvingNetworkArgsFinal {
    pub rpc_url: String,
    pub l1_rpc_url: String,
    pub fermah_address: String,
    pub lagrange_address: String,
    pub usdc_address: String,
    pub proof_manager_owner_address: String,
    pub proxy_owner_address: String,
    pub top_up_server_wallets: bool,
    pub top_up_network_wallets: bool,
}

impl DeployProvingNetworkArgs {
    pub fn fill_values_with_prompt(self, default_rpc_url: &str) -> DeployProvingNetworkArgsFinal {
        let rpc_url = self.rpc_url.unwrap_or_else(|| {
            Prompt::new(MSG_RPC_URL_PROMPT)
                .default(default_rpc_url)
                .ask()
        });
        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            Prompt::new(MSG_L1_RPC_URL_PROMPT)
                .default("http://127.0.0.1:8545")
                .ask()
        });
        let fermah_address = self.fermah_address.unwrap_or_else(|| {
            Prompt::new(MSG_FERMAH_ADDRESS_PROMPT)
                .default("0x0000000000000000000000000000000000000001")
                .ask()
        });
        let lagrange_address = self.lagrange_address.unwrap_or_else(|| {
            Prompt::new(MSG_LAGRANGE_ADDRESS_PROMPT)
                .default("0x0000000000000000000000000000000000000001")
                .ask()
        });
        let usdc_address = self.usdc_address.unwrap_or_else(|| {
            Prompt::new(MSG_USDC_ADDRESS_PROMPT)
                .default("0x0000000000000000000000000000000000000001")
                .ask()
        });
        let proof_manager_owner_address = self.proof_manager_owner_address.unwrap_or_else(|| {
            Prompt::new(MSG_PROOF_MANAGER_OWNER_ADDRESS_PROMPT)
                .default("0x0000000000000000000000000000000000000001")
                .ask()
        });
        let proxy_owner_address = self.proxy_owner_address.unwrap_or_else(|| {
            Prompt::new(MSG_PROXY_OWNER_ADDRESS_PROMPT)
                .default("0x0000000000000000000000000000000000000001")
                .ask()
        });

        let top_up_server_wallets = self.top_up_server_wallets.unwrap_or_else(|| {
            PromptConfirm::new(MSG_TOP_UP_SERVER_WALLETS_PROMPT)
                .default(true)
                .ask()
        });

        let top_up_network_wallets = self.top_up_network_wallets.unwrap_or_else(|| {
            PromptConfirm::new(MSG_TOP_UP_NETWORK_WALLETS_PROMPT)
                .default(true)
                .ask()
        });

        DeployProvingNetworkArgsFinal {
            rpc_url,
            l1_rpc_url,
            fermah_address,
            lagrange_address,
            usdc_address,
            proof_manager_owner_address,
            proxy_owner_address,
            top_up_server_wallets,
            top_up_network_wallets,
        }
    }
}
