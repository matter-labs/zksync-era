use std::{collections::HashMap, sync::Arc};

/// Consensus registry contract operations.
/// Includes code duplicated from `zksync_node_consensus::registry::abi`.
use anyhow::Context as _;
use common::config::global_config;
use config::EcosystemConfig;
use ethers::{
    contract::Multicall,
    middleware::{Middleware as _, SignerMiddleware},
    providers::{Http, Provider, RawCall as _},
    signers::{LocalWallet, Signer as _},
    types::BlockId,
};
use xshell::Shell;
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::{attester, validator};

use crate::{messages, utils::consensus::parse_attester_committee};

mod abi {
    include!(concat!(env!("OUT_DIR"), "/consensus_registry_abi.rs"));
}

fn decode_attester_key(k: &abi::Secp256K1PublicKey) -> anyhow::Result<attester::PublicKey> {
    let mut x = vec![];
    x.extend(k.tag);
    x.extend(k.x);
    ByteFmt::decode(&x)
}

fn decode_weighted_attester(
    a: &abi::CommitteeAttester,
) -> anyhow::Result<attester::WeightedAttester> {
    Ok(attester::WeightedAttester {
        weight: a.weight.into(),
        key: decode_attester_key(&a.pub_key).context("key")?,
    })
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

        let chain_id = chain_config
            .get_genesis_config()
            .context("get_genesis_config()")?
            .l2_chain_id;
        let governor = chain_config
            .get_wallets_config()
            .context("get_secrets_config()")?
            .governor
            .private_key
            .context("governor private key not set")?;
        let governor = LocalWallet::from_bytes(governor.as_bytes())
            .context("LocalWallet::from_bytes()")?
            .with_chain_id(chain_id.as_u64());

        let cfg = chain_config
            .get_general_config()
            .context("get_general_config()")?;
        let l2_url = &cfg
            .api_config
            .as_ref()
            .context("api_config missing")?
            .web3_json_rpc
            .http_url;
        let provider: Provider<Http> = l2_url
            .try_into()
            .with_context(|| format!("{l2_url}.try_into::<Provider>()"))?;
        let block_id = BlockId::from(
            provider
                .get_block_number()
                .await
                .context("get_block_number")?,
        );
        let signer = Arc::new(SignerMiddleware::new(provider.clone(), governor));

        let contracts_cfg = chain_config
            .get_contracts_config()
            .context("get_contracts_config()")?;
        let mut multicall = Multicall::new_with_chain_id(
            signer.clone(),
            Some(
                contracts_cfg
                    .l2
                    .multicall3
                    .context("multicall3 contract not configured")?,
            ),
            Some(chain_id.as_u64()),
        )
        .context("Multicall::new()")?;
        let addr = contracts_cfg
            .l2
            .consensus_registry
            .context("consensus_registry address not configured")?;
        let consensus_registry = abi::ConsensusRegistry::new(addr, signer.clone());
        match self {
            Self::SetAttesterCommittee => {
                // Fetch the desired state.
                let want = (|| {
                    Some(
                        &cfg.consensus_config
                            .as_ref()?
                            .genesis_spec
                            .as_ref()?
                            .attesters,
                    )
                })()
                .context("consensus.genesis_spec.attesters missing in general.yaml")?;
                let want = parse_attester_committee(&want).context("parse_attester_committee()")?;

                // Fetch contract state.
                let n: usize = consensus_registry
                    .num_nodes()
                    .call_raw()
                    .block(block_id)
                    .await
                    .context("num_nodes()")?
                    .try_into()
                    .ok()
                    .context("overflow")?;
                multicall.block = Some(block_id);
                let node_owners: Vec<ethers::types::Address> = multicall
                    .add_calls(
                        false,
                        (0..n).map(|i| consensus_registry.node_owners(i.into())),
                    )
                    .call_array()
                    .await
                    .context("node_owners()")?;
                multicall.clear_calls();
                let nodes: Vec<abi::NodesReturn> = multicall
                    .add_calls(
                        false,
                        node_owners
                            .iter()
                            .map(|addr| consensus_registry.nodes(*addr)),
                    )
                    .call_array()
                    .await
                    .context("nodes()")?;
                multicall.clear_calls();

                let mut txs = vec![];
                let mut want: HashMap<_, _> =
                    want.iter().map(|a| (a.key.clone(), a.weight)).collect();
                for (i, node) in nodes.into_iter().enumerate() {
                    if node.attester_latest.removed {
                        continue;
                    }
                    let got = attester::WeightedAttester {
                        key: decode_attester_key(&node.attester_latest.pub_key)
                            .context("decode_attester_key")?,
                        weight: node.attester_latest.weight.into(),
                    };
                    if let Some(weight) = want.remove(&got.key) {
                        if weight != got.weight {
                            txs.push(consensus_registry.change_attester_weight(
                                node_owners[i],
                                weight.try_into().context("overflow")?,
                            ).send().await.context("send()")?.tx_hash());
                        }
                        if !node.attester_latest.active {
                            txs.push(consensus_registry.activate(node_owners[i]).send().await.context("send()")?.tx_hash());
                        }
                    } else {
                        txs.push(consensus_registry.remove(node_owners[i]).send().await.context("send()")?.tx_hash());
                    }
                }
                for (key, weight) in want {
                    let vk = validator::SecretKey::generate();
                    txs.push(consensus_registry.add(
                        ethers::types::Address::random(),
                        /*validator_weight=*/ 1,
                        encode_validator_key(&vk.public()),
                        encode_validator_pop(&vk.sign_pop()),
                        weight.try_into().context("overflow")?,
                        encode_attester_key(&key),
                    ).send().await.context("send()")?.tx_hash());
                }
                txs.push(consensus_registry.commit_attester_committee().send().await.context("send()")?.tx_hash());
                println!("{} transactions in progress",txs.len());
                for h in txs {
                    ethers::providers::PendingTransaction::new(h,&provider).await.context("awaiting transactions")?;
                }
                println!("done");
            }
            Self::GetAttesterCommittee => {
                let attesters = consensus_registry.get_attester_committee().call().await?;
                let attesters: Vec<_> = attesters
                    .iter()
                    .map(decode_weighted_attester)
                    .collect::<Result<_, _>>()
                    .context("decode_weighted_attester")?;
                let attesters = attester::Committee::new(attesters.into_iter())
                    .context("attester::Committee::new()")?;
                for a in attesters.iter() {
                    println!("{a:?}");
                }
            }
        }
        Ok(())
    }
}
