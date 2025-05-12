use std::{borrow::Borrow, collections::HashMap, path::PathBuf, sync::Arc};

/// Consensus registry contract operations.
/// Includes code duplicated from `zksync_node_consensus::registry::abi`.
use anyhow::Context as _;
use ethers::{
    abi::Detokenize,
    contract::{FunctionCall, Multicall},
    middleware::{Middleware, NonceManagerMiddleware, SignerMiddleware},
    providers::{Http, JsonRpcClient, PendingTransaction, Provider, ProviderError, RawCall as _},
    signers::{LocalWallet, Signer as _},
    types::{Address, BlockId, H256},
};
use tokio::time::MissedTickBehavior;
use xshell::Shell;
use zkstack_cli_common::{config::global_config, logger, wallets::Wallet};
use zkstack_cli_config::EcosystemConfig;
use zksync_basic_types::L2ChainId;
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::{attester, validator};

use crate::{commands::args::WaitArgs, messages, utils::consensus::read_attester_committee_yaml};

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

#[derive(clap::Args, Debug)]
#[group(required = true, multiple = false)]
pub struct SetAttesterCommitteeCommand {
    /// Sets the attester committee in the consensus registry contract to
    /// `consensus.genesis_spec.attesters` in general.yaml.
    #[clap(long)]
    from_genesis: bool,
    /// Sets the attester committee in the consensus registry contract to
    /// the committee in the yaml file.
    /// File format is definied in `commands/consensus/proto/mod.proto`.
    #[clap(long)]
    from_file: Option<PathBuf>,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Sets the attester committee in the consensus registry contract to
    /// `consensus.genesis_spec.attesters` in general.yaml.
    SetAttesterCommittee(SetAttesterCommitteeCommand),
    /// Fetches the attester committee from the consensus registry contract.
    GetAttesterCommittee,
    /// Wait until the consensus registry contract is deployed to L2 and its owner has non-zero balance
    /// (i.e., can perform operations on the registry).
    WaitForRegistry(WaitArgs),
}

/// Collection of sent transactions.
#[derive(Default)]
struct TxSet(Vec<(H256, String)>);

impl TxSet {
    /// Sends a transactions and stores the transaction hash.
    async fn send<M: 'static + Middleware, B: Borrow<M>, D: Detokenize>(
        &mut self,
        name: String,
        call: FunctionCall<B, M, D>,
    ) -> anyhow::Result<()> {
        let hash = call.send().await.with_context(|| name.clone())?.tx_hash();
        if global_config().verbose {
            logger::debug(format!("Sent transaction {name}: {hash:?}"));
        }
        self.0.push((hash, name));
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
    chain: zkstack_cli_config::ChainConfig,
    contracts: zkstack_cli_config::ContractsConfig,
    l2_chain_id: L2ChainId,
    l2_http_url: String,
    genesis_attesters: attester::Committee,
}

impl Setup {
    fn provider(&self) -> anyhow::Result<Provider<Http>> {
        let l2_url = &self.l2_http_url;
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
            Some(self.l2_chain_id.as_u64()),
        )?)
    }

    fn governor(&self) -> anyhow::Result<Wallet> {
        Ok(self
            .chain
            .get_wallets_config()
            .context("get_wallets_config()")?
            .governor)
    }

    fn signer(&self, wallet: LocalWallet) -> anyhow::Result<Arc<impl Middleware>> {
        let wallet = wallet.with_chain_id(self.l2_chain_id.as_u64());
        let provider = self.provider().context("provider()")?;
        let signer = SignerMiddleware::new(provider, wallet.clone());
        // Allows us to send next transaction without waiting for the previous to complete.
        let signer = NonceManagerMiddleware::new(signer, wallet.address());
        Ok(Arc::new(signer))
    }

    async fn new(shell: &Shell) -> anyhow::Result<Self> {
        let ecosystem_config =
            EcosystemConfig::from_file(shell).context("EcosystemConfig::from_file()")?;
        let chain = ecosystem_config
            .load_current_chain()
            .context(messages::MSG_CHAIN_NOT_INITIALIZED)?;
        let contracts = chain
            .get_contracts_config()
            .context("get_contracts_config()")?;
        let l2_chain_id = chain
            .get_genesis_config()
            .await
            .context("get_genesis_config()")?
            .l2_chain_id()?;

        let general = chain
            .get_general_config()
            .await
            .context("get_general_config()")?;
        // We're getting a parent path here, since we need object input with the `attesters` array
        let genesis_attesters = general
            .raw_consensus_genesis_spec()
            .context(messages::MSG_CONSENSUS_GENESIS_SPEC_ATTESTERS_MISSING_IN_GENERAL_YAML)?
            .clone();
        let genesis_attesters = read_attester_committee_yaml(genesis_attesters)?;

        Ok(Self {
            chain,
            contracts,
            l2_chain_id,
            l2_http_url: general.l2_http_url()?,
            genesis_attesters,
        })
    }

    fn consensus_registry_addr(&self) -> anyhow::Result<Address> {
        self.contracts
            .l2
            .consensus_registry
            .context(messages::MSG_CONSENSUS_REGISTRY_ADDRESS_NOT_CONFIGURED)
    }

    fn consensus_registry<M: Middleware>(
        &self,
        m: Arc<M>,
    ) -> anyhow::Result<abi::ConsensusRegistry<M>> {
        let addr = self.consensus_registry_addr()?;
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

    fn read_attester_committee(
        &self,
        opts: &SetAttesterCommitteeCommand,
    ) -> anyhow::Result<attester::Committee> {
        // Fetch the desired state.
        if let Some(path) = &opts.from_file {
            let yaml = std::fs::read_to_string(path).context("read_to_string()")?;
            let yaml = serde_yaml::from_str(&yaml).context("parse YAML")?;
            return read_attester_committee_yaml(yaml);
        }
        Ok(self.genesis_attesters.clone())
    }

    async fn wait_for_registry_contract_inner(
        &self,
        args: &WaitArgs,
        verbose: bool,
    ) -> anyhow::Result<()> {
        let addr = self.consensus_registry_addr()?;
        let provider = self.provider().context("provider()")?;
        let mut interval = tokio::time::interval(args.poll_interval());
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        if verbose {
            logger::debug(messages::msg_wait_consensus_registry_started_polling(
                addr,
                provider.url(),
            ));
        }

        loop {
            interval.tick().await;

            let code = match provider.get_code(addr, None).await {
                Ok(code) => code,
                Err(ProviderError::HTTPError(err)) if err.is_connect() || err.is_timeout() => {
                    continue;
                }
                Err(err) => {
                    return Err(anyhow::Error::new(err)
                        .context(messages::MSG_CONSENSUS_REGISTRY_POLL_ERROR));
                }
            };
            if !code.is_empty() {
                logger::info(messages::msg_consensus_registry_wait_success(
                    addr,
                    code.len(),
                ));
                break;
            }
        }

        if verbose {
            logger::debug(format!("Registry contract was deployed on {addr:?}"));
        }

        let provider = Arc::new(provider);
        let registry = self
            .consensus_registry(provider.clone())
            .context("consensus_registry")?;
        let registry_owner = registry.owner().call().await.context("registry.owner()")?;
        if verbose {
            logger::debug(format!(
                "Registry contract has an owner: {registry_owner:?}"
            ));
        }
        loop {
            let balance = provider
                .get_balance(registry_owner, None)
                .await
                .context("get_balance(registry_owner)")?;
            if !balance.is_zero() {
                logger::debug(format!(
                    "Registry contract owner has non-zero balance: {balance}"
                ));
                break;
            }
            interval.tick().await;
        }
        Ok(())
    }

    async fn wait_for_registry_contract(
        &self,
        args: &WaitArgs,
        verbose: bool,
    ) -> anyhow::Result<()> {
        args.poll_with_timeout(
            messages::MSG_CONSENSUS_REGISTRY_WAIT_COMPONENT,
            self.wait_for_registry_contract_inner(args, verbose),
        )
        .await
    }

    async fn set_attester_committee(&self, want: &attester::Committee) -> anyhow::Result<()> {
        if global_config().verbose {
            logger::debug(format!("Setting attester committee: {want:?}"));
        }

        let provider = self.provider().context("provider()")?;
        let block_id = self.last_block(&provider).await.context("last_block()")?;
        if global_config().verbose {
            logger::debug(format!("Fetched latest L2 block: {block_id:?}"));
        }

        let governor = self.governor().context("governor()")?;
        if global_config().verbose {
            logger::debug(format!("Using governor: {:?}", governor.address));
        }

        let signer = self.signer(
            governor
                .private_key
                .clone()
                .context(messages::MSG_GOVERNOR_PRIVATE_KEY_NOT_SET)?,
        )?;
        let consensus_registry = self
            .consensus_registry(signer.clone())
            .context("consensus_registry()")?;
        let mut multicall = self.multicall(signer).context("multicall()")?;
        if global_config().verbose {
            logger::debug(format!(
                "Using consensus registry at {:?}, multicall at {:?}",
                consensus_registry.address(),
                multicall.contract.address()
            ));
        }

        let owner = consensus_registry.owner().call().await.context("owner()")?;
        if owner != governor.address {
            anyhow::bail!(
                "governor ({:#x}) is different than the consensus registry owner ({:#x})",
                governor.address,
                owner
            );
        }

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
        if global_config().verbose {
            logger::debug(format!(
                "Fetched number of nodes from consensus registry: {n}"
            ));
        }

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
        if global_config().verbose {
            logger::debug(format!(
                "Fetched node owners from consensus registry: {node_owners:?}"
            ));
        }

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
        if global_config().verbose {
            logger::debug(format!(
                "Fetched node info from consensus registry: {nodes:?}"
            ));
        }

        // Update the state.
        let mut txs = TxSet::default();
        let mut to_insert: HashMap<_, _> = want.iter().map(|a| (a.key.clone(), a.weight)).collect();
        for (i, node) in nodes.into_iter().enumerate() {
            if node.attester_latest.removed {
                continue;
            }

            let node_owner = node_owners[i];
            let got = attester::WeightedAttester {
                key: decode_attester_key(&node.attester_latest.pub_key)
                    .context("decode_attester_key()")?,
                weight: node.attester_latest.weight.into(),
            };

            if let Some(weight) = to_insert.remove(&got.key) {
                if weight != got.weight {
                    txs.send(
                        format!(
                            "change_attester_weight({node_owner:?}, {} -> {weight})",
                            got.weight
                        ),
                        consensus_registry.change_attester_weight(
                            node_owners[i],
                            weight.try_into().context("weight overflow")?,
                        ),
                    )
                    .await?;
                }
                if !node.attester_latest.active {
                    txs.send(
                        format!("activate({node_owner:?})"),
                        consensus_registry.activate(node_owner),
                    )
                    .await?;
                }
            } else {
                txs.send(
                    format!("remove({node_owner:?})"),
                    consensus_registry.remove(node_owner),
                )
                .await?;
            }
        }
        for (key, weight) in to_insert {
            let vk = validator::SecretKey::generate();
            txs.send(
                format!("add({key:?}, {weight})"),
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
            "commit_attester_committee".to_owned(),
            consensus_registry.commit_attester_committee(),
        )
        .await?;
        txs.wait(&provider).await.context("wait()")?;
        Ok(())
    }
}

impl Command {
    pub(crate) async fn run(self, shell: &Shell) -> anyhow::Result<()> {
        let setup = Setup::new(shell).await?;
        match self {
            Self::SetAttesterCommittee(opts) => {
                let want = setup
                    .read_attester_committee(&opts)
                    .context("read_attester_committee()")?;
                setup.set_attester_committee(&want).await?;
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
            Self::WaitForRegistry(args) => {
                let verbose = global_config().verbose;
                setup.wait_for_registry_contract(&args, verbose).await?;
            }
        }
        Ok(())
    }
}
