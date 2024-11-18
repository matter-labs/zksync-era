use std::str::FromStr;

use anyhow::Context;
use clap::Parser;
use common::{
    forge::{ForgeScriptArg, ForgeScriptArgs},
    wallets::Wallet,
    Prompt,
};
use config::EcosystemConfig;
use ethers::{abi::Address, prelude::LocalWallet, types::H256};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    defaults::LOCAL_RPC_URL,
    messages::{MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT},
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct ProposeRegistrationArgs {
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
    #[clap(long)]
    pub chain_registrar: Option<Address>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long)]
    pub broadcast: bool,
    #[clap(long)]
    pub dev: bool,
    #[clap(long)]
    pub private_key: Option<H256>,
    #[clap(long)]
    pub sender: Option<Address>,
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
            let l1_rpc_url = self.l1_rpc_url.unwrap_or(LOCAL_RPC_URL.to_string());
            let chain_registrar = if let Some(chain_registrar) = self.chain_registrar {
                chain_registrar
            } else {
                chain_registrar_default.context("Ecosystem must be provided for dev mode")?
            };

            let main_wallet = if let Some(pk) = self.private_key {
                let local_wallet = LocalWallet::from_bytes(pk.as_bytes())?;
                Wallet::new(local_wallet)
            } else {
                config
                    .map(|config| config.get_wallets().map(|wallets| wallets.governor))
                    .transpose()?
                    .context("Ecosystem must be provided for dev mode")?
            };

            return Ok(ProposeRegistrationArgsFinal {
                l1_rpc_url,
                chain_registrar,
                main_wallet,
                broadcast: true,
                forge_args: Default::default(),
                sender: None,
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

        let pk = self
            .private_key
            .unwrap_or_else(|| Prompt::new("Please specify your private key").ask());
        let local_wallet = LocalWallet::from_bytes(pk.as_bytes())?;

        Ok(ProposeRegistrationArgsFinal {
            l1_rpc_url,
            chain_registrar,
            main_wallet: Wallet::new(local_wallet),
            broadcast: self.broadcast,
            forge_args: self.forge_args,
            sender: self.sender,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ProposeRegistrationArgsFinal {
    pub l1_rpc_url: String,
    pub chain_registrar: Address,
    pub main_wallet: Wallet,
    pub broadcast: bool,
    pub forge_args: ForgeScriptArgs,
    pub sender: Option<Address>,
}
