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

use crate::{
    commands::args::WaitArgs,
    messages,
    utils::consensus::{read_validator_committee_yaml, Validator},
};

#[allow(warnings)]
mod abi {
    include!(concat!(env!("OUT_DIR"), "/consensus_registry_abi.rs"));
}

#[derive(clap::Args, Debug)]
#[group(required = true)]
pub struct SetValidatorCommitteeCommand {
    /// Sets the validator committee in the consensus registry contract to
    /// the committee in the yaml file.
    /// File format is defined in `SetValidatorCommitteeFile` in this crate.
    #[clap(long)]
    from_file: PathBuf,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Sets the validator committee in the consensus registry contract to
    /// the committee in the yaml file.
    /// File format is defined in `SetValidatorCommitteeFile` in this crate.
    SetValidatorCommittee(SetValidatorCommitteeCommand),
    /// Fetches the validator committee from the consensus registry contract.
    GetValidatorCommittee,
    /// Wait until the consensus registry contract is deployed to L2.
    WaitForRegistry(WaitArgs),
}

impl Command {
    pub(crate) async fn run(self, shell: &Shell) -> anyhow::Result<()> {
        let setup = Setup::new(shell).await?;
        match self {
            Self::SetValidatorCommittee(opts) => {
                let (want, validators) = setup
                    .read_validator_committee_file(&opts)
                    .context("read_validator_committee()")?;
                setup.set_validator_committee(validators).await?;
                let got = setup.get_validator_committee().await?;
                anyhow::ensure!(
                    got == want,
                    messages::msg_setting_validator_committee_failed(&got, &want)
                );
                print_validators(&got);
            }
            Self::GetValidatorCommittee => {
                let got = setup.get_validator_committee().await?;
                print_validators(&got);
            }
            Self::WaitForRegistry(args) => {
                let verbose = global_config().verbose;
                setup.wait_for_registry_contract(&args, verbose).await?;
            }
        }
        Ok(())
    }
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

struct Setup {
    chain: zkstack_cli_config::ChainConfig,
    contracts: zkstack_cli_config::ContractsConfig,
    l2_chain_id: L2ChainId,
    l2_http_url: String,
}

impl Setup {
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
            .l2_chain_id()
            .context("l2_chain_id()")?;
        let l2_http_url = chain
            .get_general_config()
            .await
            .context("get_general_config()")?
            .l2_http_url()
            .context("l2_http_url()")?;

        Ok(Self {
            chain,
            contracts,
            l2_chain_id,
            l2_http_url,
        })
    }

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

    fn read_validator_committee_file(
        &self,
        opts: &SetValidatorCommitteeCommand,
    ) -> anyhow::Result<(validator::Committee, Vec<Validator>)> {
        let yaml = std::fs::read_to_string(opts.from_file.clone()).context("read_to_string()")?;
        let yaml = serde_yaml::from_str(&yaml).context("parse YAML")?;
        read_validator_committee_yaml(yaml)
    }

    async fn last_block(&self, m: &(impl 'static + Middleware)) -> anyhow::Result<BlockId> {
        Ok(m.get_block_number()
            .await
            .context("get_block_number()")?
            .into())
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

    async fn get_validator_committee(&self) -> anyhow::Result<validator::Committee> {
        let provider = Arc::new(self.provider()?);
        let consensus_registry = self
            .consensus_registry(provider)
            .context("consensus_registry()")?;
        let validators = consensus_registry
            .get_validator_committee()
            .call()
            .await
            .context("get_validator_committee()")?;
        let validators: Vec<_> = validators
            .iter()
            .map(decode_weighted_validator)
            .collect::<Result<_, _>>()
            .context("decode_weighted_validator()")?;
        validator::Committee::new(validators.into_iter()).context("validator::Committee::new()")
    }

    async fn set_validator_committee(&self, validators: Vec<Validator>) -> anyhow::Result<()> {
        if global_config().verbose {
            logger::debug(format!("Setting validator committee: {validators:?}"));
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
        let mut to_insert: HashMap<_, _> = validators
            .iter()
            .map(|a| (a.key.clone(), (a.pop.clone(), a.weight)))
            .collect();

        for (i, node) in nodes.into_iter().enumerate() {
            if node.validator_latest.removed {
                continue;
            }

            let node_owner = node_owners[i];
            let got = validator::WeightedValidator {
                key: decode_validator_key(&node.validator_latest.pub_key)
                    .context("decode_validator_key()")?,
                weight: node.validator_latest.weight.into(),
            };

            if let Some((_pop, weight)) = to_insert.remove(&got.key) {
                if weight != got.weight {
                    txs.send(
                        format!(
                            "change_validator_weight({node_owner:?}, {} -> {weight})",
                            got.weight
                        ),
                        consensus_registry.change_validator_weight(
                            node_owners[i],
                            weight.try_into().context("weight overflow")?,
                        ),
                    )
                    .await?;
                }
                if !node.validator_latest.active {
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

        for (key, (pop, weight)) in to_insert {
            // We are inserting dummy attester keys here since we won't use them.
            let ak = attester::SecretKey::generate();
            txs.send(
                format!("add({key:?}, {weight})"),
                consensus_registry.add(
                    Address::random(),
                    weight.try_into().context("overflow")?,
                    encode_validator_key(&key),
                    encode_validator_pop(&pop),
                    1,
                    encode_attester_key(&ak.public()),
                ),
            )
            .await?;
        }
        txs.send(
            "commit_validator_committee".to_owned(),
            consensus_registry.commit_validator_committee(),
        )
        .await?;
        txs.wait(&provider).await.context("wait()")?;
        Ok(())
    }
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

fn decode_validator_key(k: &abi::Bls12381PublicKey) -> anyhow::Result<validator::PublicKey> {
    let mut x = vec![];
    x.extend(k.a);
    x.extend(k.b);
    x.extend(k.c);
    ByteFmt::decode(&x)
}

fn decode_weighted_validator(
    v: &abi::CommitteeValidator,
) -> anyhow::Result<validator::WeightedValidator> {
    Ok(validator::WeightedValidator {
        weight: v.weight.into(),
        key: decode_validator_key(&v.pub_key).context("key")?,
    })
}

fn print_validators(committee: &validator::Committee) {
    logger::success(
        committee
            .iter()
            .map(|a| format!("{a:?}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
