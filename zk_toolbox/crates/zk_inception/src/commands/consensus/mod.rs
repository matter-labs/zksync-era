use anyhow::Context as _;
use xshell::Shell;
use ethers::providers::{Provider, Http};
use ethers::middleware::SignerMiddleware;
use ethers::signers::{Signer as _, LocalWallet};
use config::EcosystemConfig;
use common::config::global_config;
use std::sync::Arc;
use crate::messages;

mod abi {
    include!(concat!(env!("OUT_DIR"), "/consensus_registry_abi.rs"));
}

#[derive(Debug, clap::Parser)]
pub struct AddArgs {
    
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    Add(AddArgs),
    CommitAttesterCommittee,
    GetAttesterCommittee,
}

impl Command {
    pub(crate) async fn run(self, shell: &Shell) -> anyhow::Result<()> {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        let chain_name = global_config().chain_name.clone();
        let chain_config = ecosystem_config
            .load_chain(chain_name)
            .context(messages::MSG_CHAIN_NOT_INITIALIZED)?;
        
        let chain_id = chain_config.get_genesis_config().context("get_genesis_config()")?.l2_chain_id;
        let governor = chain_config.get_wallets_config().context("get_secrets_config()")?.governor.private_key.context("governor private key not set")?;
        let governor = LocalWallet::from_bytes(governor.as_bytes()).context("LocalWallet::from_bytes()")?.with_chain_id(chain_id.as_u64());

        let cfg = chain_config.get_general_config().context("get_general_config()")?;
        let l2_url = &cfg.api_config.as_ref().context("api_config missing")?.web3_json_rpc.http_url;
        let provider : Provider<Http> = l2_url.try_into().with_context(||format!("{l2_url}.try_into::<Provider>()"))?;
        let provider = Arc::new(SignerMiddleware::new(provider, governor));
        
        let contracts_cfg = chain_config.get_contracts_config().context("get_contracts_config()")?;
        let addr = contracts_cfg.l2.consensus_registry.context("consensus_registry address not configured")?; 
        let consensus_registry = abi::ConsensusRegistry::new(addr, provider);
        match self {
            Self::Add(args) => {

            }
            Self::CommitAttesterCommittee => {
                let res = consensus_registry.commit_attester_committee()
                    .send().await.context("send()")?
                    .await.context("awaiting transaction")?;
                println!("result = {res:?}");
            }
            Self::GetAttesterCommittee => {
                let attesters = consensus_registry.get_attester_committee().call().await?;
                println!("attesters = {attesters:?}");
            }
        }
        Ok(())
    }
}
