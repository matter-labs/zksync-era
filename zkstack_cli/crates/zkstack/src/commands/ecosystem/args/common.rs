use clap::Parser;
use ethers::middleware::Middleware;
use serde::{Deserialize, Serialize};
use url::Url;
use zkstack_cli_common::{ethereum::get_ethers_provider, logger, Prompt};
use zkstack_cli_types::{L1Network, VMOption};

use crate::{
    defaults::LOCAL_RPC_URL,
    messages::{MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_RPC_URL_PROMPT},
};

#[derive(Parser, Debug, Clone, Serialize, Deserialize)]
pub struct CommonEcosystemArgs {
    #[clap(long, default_value_t = false, default_missing_value = "true")]
    pub(crate) zksync_os: bool,
    #[clap(long, default_value_t = true, default_missing_value = "true", num_args = 0..=1)]
    pub(crate) update_submodules: bool,
    #[clap(long, default_value_t = false, default_missing_value = "true", num_args = 0..=1)]
    pub(crate) skip_contract_compilation_override: bool,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub(crate) l1_rpc_url: Option<String>,
}

impl CommonEcosystemArgs {
    pub async fn fill_values_with_prompt(
        self,
        l1_network: L1Network,
        dev: bool,
    ) -> anyhow::Result<CommonEcosystemFinalArgs> {
        let l1_rpc_url = self.l1_rpc_url.clone().unwrap_or_else(|| {
            let mut prompt = Prompt::new(MSG_RPC_URL_PROMPT);
            if dev {
                return LOCAL_RPC_URL.to_string();
            }
            if l1_network == L1Network::Localhost {
                prompt = prompt.default(LOCAL_RPC_URL);
            }
            prompt
                .validate_with(|val: &String| -> Result<(), String> {
                    Url::parse(val)
                        .map(|_| ())
                        .map_err(|_| MSG_L1_RPC_URL_INVALID_ERR.to_string())
                })
                .ask()
        });

        check_l1_rpc_health(&l1_rpc_url).await?;

        Ok(CommonEcosystemFinalArgs {
            vm_option: self.vm_option(),
            l1_rpc_url,
        })
    }

    pub fn vm_option(&self) -> VMOption {
        if self.zksync_os {
            VMOption::ZKSyncOsVM
        } else {
            VMOption::EraVM
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommonEcosystemFinalArgs {
    pub(crate) vm_option: VMOption,
    pub(crate) l1_rpc_url: String,
}

/// Check if L1 RPC is healthy by calling eth_chainId
async fn check_l1_rpc_health(l1_rpc_url: &str) -> anyhow::Result<()> {
    // Check L1 RPC health after getting the URL
    logger::info("🔍 Checking L1 RPC health...");
    let l1_provider = get_ethers_provider(l1_rpc_url)?;
    let l1_chain_id = l1_provider.get_chainid().await?.as_u64();

    logger::info(format!(
        "✅ L1 RPC health check passed - chain ID: {}",
        l1_chain_id
    ));
    Ok(())
}
