use std::{borrow::Borrow, collections::HashMap, sync::Arc};

/// Consensus registry contract operations.
/// Includes code duplicated from `zksync_node_consensus::registry::abi`.
use anyhow::Context as _;
use common::logger;
use config::EcosystemConfig;
use ethers::{
    abi::Detokenize,
    contract::{FunctionCall, Multicall},
    middleware::{Middleware, NonceManagerMiddleware, SignerMiddleware},
    providers::{Http, JsonRpcClient, PendingTransaction, Provider, RawCall as _},
    signers::{LocalWallet, Signer as _},
    types::{Address, BlockId, H256},
};
use xshell::Shell;
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::{attester, validator};

use crate::{messages, utils::consensus::parse_attester_committee};

#[allow(warnings)]
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
    /// Sets the attester committee in the consensus registry contract to
    /// `consensus.genesis_spec.attesters` in general.yaml.
    SetAttesterCommittee,
    /// Fetches the attester committee from the consensus registry contract.
    GetAttesterCommittee,
}

/// Collection of sent transactions.
#[derive(Default)]
pub struct TxSet(Vec<(H256, &'static str)>);

impl TxSet {
    /// Sends a transactions and stores the transaction hash.
    pub async fn send<M: 'static + Middleware, B: Borrow<M>, D: Detokenize>(
        &mut self,
        name: &'static str,
        call: FunctionCall<B, M, D>,
    ) -> anyhow::Result<()> {
        let h = call.send().await.context(name)?.tx_hash();
        self.0.push((h, name));
        Ok(())
    }

    /// Waits for all stored transactions to complete.
    pub async fn wait<P: JsonRpcClient>(self, provider: &Provider<P>) -> anyhow::Result<()> {
        for (h, name) in self.0 {
            async {
                let status = PendingTransaction::new(h, provider)
                    .await
                    .context("await")?
                    .context(messages::MSG_RECEIPT_MISSING)?
                    .status
                    .context(messages::MSG_STATUS_MISSING)?;
                anyhow::ensure!(status == 1.into(), messages::MSG_TRANSACTION_FAILED);
                Ok(())
            }
            .await
            .context(name)?;
        }
        Ok(())
    }
}

fn print_attesters(committee: &attester::Committee) {
    logger::success(
        committee
            .iter()
            .map(|a| format!("{a:?}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

struct Setup {
    chain: config::ChainConfig,
    contracts: config::ContractsConfig,
    general: config::GeneralConfig,
    genesis: config::GenesisConfig,
}

impl Setup {
    fn provider(&self) -> anyhow::Result<Provider<Http>> {
        let l2_url = &self
            .general
            .api_config
            .as_ref()
            .context(messages::MSG_API_CONFIG_MISSING)?
            .web3_json_rpc
            .http_url;
        Provider::try_from(l2_url).with_context(|| format!("Provider::try_from({l2_url})"))
    }

    fn multicall<M: 'static + Middleware>(&self, m: Arc<M>) -> anyhow::Result<Multicall<M>> {
        Ok(Multicall::new_with_chain_id(
            m,
            Some(
                self.chain
                    .get_contracts_config()
                    .context("get_contracts_config()")?
                    .l2
                    .multicall3
                    .context(messages::MSG_MULTICALL3_CONTRACT_NOT_CONFIGURED)?,
            ),
            Some(self.genesis.l2_chain_id.as_u64()),
        )?)
    }

    fn governor(&self) -> anyhow::Result<Arc<impl Middleware>> {
        let governor = self
            .chain
            .get_wallets_config()
            .context("get_wallets_config()")?
            .governor
            .private_key
            .context(messages::MSG_GOVERNOR_PRIVATE_KEY_NOT_SET)?;
        let governor = LocalWallet::from_bytes(governor.as_bytes())
            .context("LocalWallet::from_bytes()")?
            .with_chain_id(self.genesis.l2_chain_id.as_u64());
        let provider = self.provider().context("provider()")?;
        let signer = SignerMiddleware::new(provider, governor.clone());
        // Allows us to send next transaction without waiting for the previous to complete.
        let signer = NonceManagerMiddleware::new(signer, governor.address());
        Ok(Arc::new(signer))
    }

    fn new(shell: &Shell) -> anyhow::Result<Self> {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        let chain = ecosystem_config
            .load_current_chain()
            .context(messages::MSG_CHAIN_NOT_INITIALIZED)?;
        let contracts = chain
            .get_contracts_config()
            .context("get_contracts_config()")?;
        let genesis = chain.get_genesis_config().context("get_genesis_config()")?;
        let general = chain.get_general_config().context("get_general_config()")?;
        Ok(Self {
            chain,
            contracts,
            general,
            genesis,
        })
    }

    fn consensus_registry<M: Middleware>(
        &self,
        m: Arc<M>,
    ) -> anyhow::Result<abi::ConsensusRegistry<M>> {
        let addr = self
            .contracts
            .l2
            .consensus_registry
            .context(messages::MSG_CONSENSUS_REGISTRY_ADDRESS_NOT_CONFIGURED)?;
        Ok(abi::ConsensusRegistry::new(addr, m))
    }

    async fn last_block(&self, m: &(impl 'static + Middleware)) -> anyhow::Result<BlockId> {
        Ok(m.get_block_number()
            .await
            .context("get_block_number()")?
            .into())
    }

    async fn get_attester_committee(&self) -> anyhow::Result<attester::Committee> {
        let provider = Arc::new(self.provider()?);
        let consensus_registry = self
            .consensus_registry(provider)
            .context("consensus_registry()")?;
        let attesters = consensus_registry
            .get_attester_committee()
            .call()
            .await
            .context("get_attester_committee()")?;
        let attesters: Vec<_> = attesters
            .iter()
            .map(decode_weighted_attester)
            .collect::<Result<_, _>>()
            .context("decode_weighted_attester()")?;
        attester::Committee::new(attesters.into_iter()).context("attester::Committee::new()")
    }

    async fn set_attester_committee(&self) -> anyhow::Result<attester::Committee> {
        // Fetch the desired state.
        let want = (|| {
            Some(
                &self
                    .general
                    .consensus_config
                    .as_ref()?
                    .genesis_spec
                    .as_ref()?
                    .attesters,
            )
        })()
        .context(messages::MSG_CONSENSUS_GENESIS_SPEC_ATTESTERS_MISSING_IN_GENERAL_YAML)?;
        let want = parse_attester_committee(want).context("parse_attester_committee()")?;

        let provider = self.provider().context("provider()")?;
        let block_id = self.last_block(&provider).await.context("last_block()")?;
        let governor = self.governor().context("governor()")?;
        let consensus_registry = self
            .consensus_registry(governor.clone())
            .context("consensus_registry()")?;
        let mut multicall = self.multicall(governor.clone()).context("multicall()")?;

        // Fetch contract state.
        let n: usize = consensus_registry
            .num_nodes()
            .call_raw()
            .block(block_id)
            .await
            .context("num_nodes()")?
            .try_into()
            .ok()
            .context("num_nodes() overflow")?;

        multicall.block = Some(block_id);
        let node_owners: Vec<Address> = multicall
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

        // Update the state.
        let mut txs = TxSet::default();
        let mut to_insert: HashMap<_, _> = want.iter().map(|a| (a.key.clone(), a.weight)).collect();
        for (i, node) in nodes.into_iter().enumerate() {
            if node.attester_latest.removed {
                continue;
            }
            let got = attester::WeightedAttester {
                key: decode_attester_key(&node.attester_latest.pub_key)
                    .context("decode_attester_key()")?,
                weight: node.attester_latest.weight.into(),
            };
            if let Some(weight) = to_insert.remove(&got.key) {
                if weight != got.weight {
                    txs.send(
                        "changed_attester_weight",
                        consensus_registry.change_attester_weight(
                            node_owners[i],
                            weight.try_into().context("weight overflow")?,
                        ),
                    )
                    .await?;
                }
                if !node.attester_latest.active {
                    txs.send("activate", consensus_registry.activate(node_owners[i]))
                        .await?;
                }
            } else {
                txs.send("remove", consensus_registry.remove(node_owners[i]))
                    .await?;
            }
        }
        for (key, weight) in to_insert {
            let vk = validator::SecretKey::generate();
            txs.send(
                "add",
                consensus_registry.add(
                    Address::random(),
                    /*validator_weight=*/ 1,
                    encode_validator_key(&vk.public()),
                    encode_validator_pop(&vk.sign_pop()),
                    weight.try_into().context("overflow")?,
                    encode_attester_key(&key),
                ),
            )
            .await?;
        }
        txs.send(
            "commit_attester_committee",
            consensus_registry.commit_attester_committee(),
        )
        .await?;
        txs.wait(&provider).await.context("wait()")?;
        Ok(want)
    }
}

impl Command {
    pub(crate) async fn run(self, shell: &Shell) -> anyhow::Result<()> {
        let setup = Setup::new(shell).context("Setup::new()")?;
        match self {
            Self::SetAttesterCommittee => {
                let want = setup.set_attester_committee().await?;
                let got = setup.get_attester_committee().await?;
                anyhow::ensure!(
                    got == want,
                    messages::msg_setting_attester_committee_failed(&got, &want)
                );
                print_attesters(&got);
            }
            Self::GetAttesterCommittee => {
                let got = setup.get_attester_committee().await?;
                print_attesters(&got);
            }
        }
        Ok(())
    }
}
