use anyhow::Context as _;
use xshell::Shell;
use ethers::providers::{Provider, Http};
use ethers::middleware::SignerMiddleware;
use ethers::signers::{Signer as _, LocalWallet};
use config::EcosystemConfig;
use common::config::global_config;
use std::sync::Arc;
use crate::messages;
use zksync_consensus_roles::{attester,validator};
use zksync_consensus_crypto::{TextFmt, ByteFmt};

mod abi {
    include!(concat!(env!("OUT_DIR"), "/consensus_registry_abi.rs"));
}

fn decode_attester_key(k : &abi::Secp256K1PublicKey) -> anyhow::Result<attester::PublicKey> {
    let mut x = vec![];
    x.extend(k.tag);
    x.extend(k.x);
    ByteFmt::decode(&x)
}

fn decode_weighted_attester(a: &abi::CommitteeAttester) -> anyhow::Result<attester::WeightedAttester> {
    Ok(attester::WeightedAttester {
        weight: a.weight.into(),
        key: decode_attester_key(&a.pub_key).context("key")?,
    })
}

pub(crate) struct WeightedValidator {
    weight: validator::Weight,
    key: validator::PublicKey,
    pop: validator::ProofOfPossession,
}

fn encode_attester_key(k: &attester::PublicKey) -> abi::Secp256K1PublicKey {
    let b: [u8; 33] = ByteFmt::encode(k).try_into().unwrap();
    abi::Secp256K1PublicKey {
        tag: b[0..1].try_into().unwrap(),
        x: b[1..33].try_into().unwrap(),
    }
}

fn encode_validator_key(k: &validator::PublicKey) -> abi::Bls12381PublicKey {
    let b: [u8; 96] = ByteFmt::encode(k).try_into().unwrap();
    abi::Bls12381PublicKey {
        a: b[0..32].try_into().unwrap(),
        b: b[32..64].try_into().unwrap(),
        c: b[64..96].try_into().unwrap(),
    }
}

fn encode_validator_pop(pop: &validator::ProofOfPossession) -> abi::Bls12381Signature {
    let b: [u8; 48] = ByteFmt::encode(pop).try_into().unwrap();
    abi::Bls12381Signature {
        a: b[0..32].try_into().unwrap(),
        b: b[32..48].try_into().unwrap(),
    }
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    SetAttesterCommittee,
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
        let block_id = provider.get_block_number().await.context("get_block_number")?;
        let provider = Arc::new(SignerMiddleware::new(provider, governor));
       
        let contracts_cfg = chain_config.get_contracts_config().context("get_contracts_config()")?;
        let addr = contracts_cfg.l2.consensus_registry.context("consensus_registry address not configured")?; 
        let consensus_registry = abi::ConsensusRegistry::new(addr, provider);
        match self {
            Self::SetAttesterCommittee => {
                let attesters = consensus_registry.get_attester_committee().call().await?;
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
