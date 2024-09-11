use anyhow::Context as _;
use xshell::Shell;
use ethers::providers::{Provider, Http};
use config::EcosystemConfig;
use common::config::global_config;
use std::sync::Arc;
use crate::messages;

mod abi {
    include!(concat!(env!("OUT_DIR"), "/consensus_registry_abi.rs"));
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    GetAttesterCommittee,
}

impl Command {
    pub(crate) async fn run(self, shell: &Shell) -> anyhow::Result<()> {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        let chain_name = global_config().chain_name.clone();
        let chain_config = ecosystem_config
            .load_chain(chain_name)
            .context(messages::MSG_CHAIN_NOT_INITIALIZED)?;
        let cfg = chain_config.get_general_config().context("get_general_config()")?;
        let l2_url = &cfg.api_config.as_ref().context("api_config missing")?.web3_json_rpc.http_url;
        let provider = Arc::new(Provider::<Http>::try_from(l2_url).with_context(||format!("Provide::try_from({l2_url})"))?);
        let addr = ethers::core::types::Address::random(); 
        let consensus_registry = abi::ConsensusRegistry::new(addr, provider);
        match self {
            Self::GetAttesterCommittee => {
                let _attesters = consensus_registry.get_attester_committee().call().await?;
                //provider.call(tx,None).await
            }
        }
        Ok(())
    }
}
