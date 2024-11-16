use crate::defaults::LOCAL_RPC_URL;
use crate::messages::{MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT};
use anyhow::Context;
use clap::Parser;
use common::Prompt;
use config::EcosystemConfig;
use ethers::abi::Address;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct ProposeRegistrationArgs {
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
    #[clap(long)]
    pub chain_registrar: Option<Address>,
    #[clap(long)]
    pub dev: bool,
}

impl ProposeRegistrationArgs {
    pub fn fill_values_with_prompt(
        self,
        config: Option<&EcosystemConfig>,
    ) -> anyhow::Result<ProposeRegistrationArgsFinal> {
        let chain_registrar_default = config
            .map(|config| {
                config
                    .get_contracts_config()
                    .map(|contracts| contracts.ecosystem_contracts.chain_registrar)
            })
            .transpose()?;

        if self.dev {
            let l1_rpc_url = LOCAL_RPC_URL.to_string();
            let chain_registrar =
                chain_registrar_default.context("Ecosystem must be provided for dev mode")?;
            return Ok(ProposeRegistrationArgsFinal {
                l1_rpc_url,
                chain_registrar,
            });
        }

        let chain_registrar = self.chain_registrar.unwrap_or_else(|| {
            let mut prompt = Prompt::new("Provide chain registrar for the the ecosystem");
            if let Some(chain_registrar) = chain_registrar_default {
                prompt = prompt.default(&format!("{:?}", chain_registrar))
            };
            prompt.ask()
        });

        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            let mut prompt = Prompt::new(MSG_L1_RPC_URL_PROMPT);
            prompt = prompt.default(LOCAL_RPC_URL);
            prompt
                .validate_with(|val: &String| -> Result<(), String> {
                    Url::parse(val)
                        .map(|_| ())
                        .map_err(|_| MSG_L1_RPC_URL_INVALID_ERR.to_string())
                })
                .ask()
        });

        Ok(ProposeRegistrationArgsFinal {
            l1_rpc_url,
            chain_registrar,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ProposeRegistrationArgsFinal {
    pub l1_rpc_url: String,
    pub chain_registrar: Address,
}
