use clap::Parser;
use serde::{Deserialize, Serialize};
use zkstack_cli_common::Prompt;

use crate::messages::{
    MSG_FERMAH_ADDRESS_PROMPT, MSG_LAGRANGE_ADDRESS_PROMPT, MSG_PROOF_MANAGER_OWNER_ADDRESS_PROMPT,
    MSG_PROXY_OWNER_ADDRESS_PROMPT, MSG_RPC_URL_PROMPT, MSG_USDC_ADDRESS_PROMPT,
};

#[derive(Debug, Clone, Parser, Default, Serialize, Deserialize)]
pub struct DeployProvingNetworkArgs {
    #[clap(long)]
    pub rpc_url: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployProvingNetworkArgsFinal {
    pub rpc_url: String,
    pub fermah_address: String,
    pub lagrange_address: String,
    pub usdc_address: String,
    pub proof_manager_owner_address: String,
    pub proxy_owner_address: String,
}

impl DeployProvingNetworkArgs {
    pub fn fill_values_with_prompt(self, default_rpc_url: &str) -> DeployProvingNetworkArgsFinal {
        let rpc_url = self.rpc_url.unwrap_or_else(|| {
            Prompt::new(MSG_RPC_URL_PROMPT)
                .default(default_rpc_url)
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

        DeployProvingNetworkArgsFinal {
            rpc_url,
            fermah_address,
            lagrange_address,
            usdc_address,
            proof_manager_owner_address,
            proxy_owner_address,
        }
    }
}
