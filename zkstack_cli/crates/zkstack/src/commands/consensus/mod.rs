use std::{borrow::Borrow, collections::HashMap, path::PathBuf, sync::Arc};

/// Consensus registry contract operations.
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
use zksync_consensus_roles::validator;

use crate::{
    commands::args::WaitArgs,
    messages,
    utils::consensus::{read_validator_schedule_yaml, LeaderSelectionInFile, ValidatorWithPop},
};

#[allow(warnings)]
mod abi {
    include!(concat!(env!("OUT_DIR"), "/consensus_registry_abi.rs"));
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Sets the validator schedule in the consensus registry contract to
    /// the schedule in the yaml file.
    /// File format is defined in `SetValidatorScheduleFile` in this crate.
    SetValidatorSchedule(SetValidatorScheduleCommand),
    /// Sets the committee activation delay in the consensus registry contract.
    SetScheduleActivationDelay(SetScheduleActivationDelayCommand),
    /// Fetches the current validator schedule from the consensus registry contract.
    GetValidatorSchedule,
    /// Fetches the pending validator schedule from the consensus registry contract, if any.
    GetPendingValidatorSchedule,
    /// Wait until the consensus registry contract is deployed to L2.
    WaitForRegistry(WaitArgs),
}

#[derive(clap::Args, Debug)]
#[group(required = true)]
pub struct SetValidatorScheduleCommand {
    /// The file to read the validator schedule from.
    #[clap(long)]
    pub from_file: PathBuf,
}

#[derive(clap::Args, Debug)]
pub struct SetScheduleActivationDelayCommand {
    /// The activation delay in blocks.
    #[clap(long)]
    pub delay: u64,
}

impl Command {
    pub(crate) async fn run(self, shell: &Shell) -> anyhow::Result<()> {
        let setup = Setup::new(shell).await?;
        match self {
            Self::SetValidatorSchedule(opts) => {
                let (schedule_want, validators_want, leader_selection_want) = setup
                    .read_validator_schedule_file(&opts)
                    .context("read_validator_schedule_file()")?;
                setup
                    .set_validator_schedule(validators_want, leader_selection_want)
                    .await?;

                // Check if the schedule was set correctly (check pending first, then current)
                let pending_schedule = setup.get_pending_validator_schedule().await?;
                let (schedule_got, is_pending) = if pending_schedule == schedule_want {
                    (pending_schedule, true)
                } else {
                    let current_schedule = setup.get_validator_schedule().await?;
                    anyhow::ensure!(
                        current_schedule == schedule_want,
                        messages::msg_setting_validator_schedule_failed(
                            &current_schedule,
                            &schedule_want
                        )
                    );
                    (current_schedule, false)
                };

                print_schedule(&schedule_got, is_pending);
            }
            Self::SetScheduleActivationDelay(opts) => {
                setup.set_schedule_activation_delay(opts.delay).await?;
                logger::success(format!(
                    "Successfully set schedule activation delay to {} blocks",
                    opts.delay
                ));
            }
            Self::GetValidatorSchedule => {
                let got = setup.get_validator_schedule().await?;
                print_schedule(&got, false);
            }
            Self::GetPendingValidatorSchedule => {
                let got = setup.get_pending_validator_schedule().await?;
                print_schedule(&got, true);
            }
            Self::WaitForRegistry(args) => {
                let verbose = global_config().verbose;
                setup.wait_for_registry_contract(&args, verbose).await?;
            }
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

    fn read_validator_schedule_file(
        &self,
        opts: &SetValidatorScheduleCommand,
    ) -> anyhow::Result<(
        validator::Schedule,
        Vec<ValidatorWithPop>,
        LeaderSelectionInFile,
    )> {
        let yaml = std::fs::read_to_string(opts.from_file.clone()).context("read_to_string()")?;
        let yaml = serde_yaml::from_str(&yaml).context("parse YAML")?;
        read_validator_schedule_yaml(yaml)
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

    async fn get_validator_schedule(&self) -> anyhow::Result<validator::Schedule> {
        let provider = Arc::new(self.provider()?);
        let consensus_registry = self
            .consensus_registry(provider)
            .context("consensus_registry()")?;
        let (validators, leader_selection) = consensus_registry
            .get_validator_committee()
            .call()
            .await
            .context("get_validator_committee()")?;
        let validators: Vec<_> = validators
            .iter()
            .map(decode_committee_validator)
            .collect::<Result<_, _>>()
            .context("decode_committee_validator()")?;
        let leader_selection =
            decode_leader_selection(&leader_selection).context("decode_leader_selection()")?;
        validator::Schedule::new(validators, leader_selection)
    }

    async fn get_pending_validator_schedule(&self) -> anyhow::Result<validator::Schedule> {
        let provider = Arc::new(self.provider()?);
        let consensus_registry = self
            .consensus_registry(provider)
            .context("consensus_registry()")?;
        let (validators, leader_selection) = consensus_registry
            .get_next_validator_committee()
            .call()
            .await
            .context("get_next_validator_committee()")?;
        let validators: Vec<_> = validators
            .iter()
            .map(decode_committee_validator)
            .collect::<Result<_, _>>()
            .context("decode_committee_validator()")?;
        let leader_selection =
            decode_leader_selection(&leader_selection).context("decode_leader_selection()")?;
        validator::Schedule::new(validators, leader_selection)
    }

    async fn set_validator_schedule(
        &self,
        validators_want: Vec<ValidatorWithPop>,
        leader_selection_want: LeaderSelectionInFile,
    ) -> anyhow::Result<()> {
        if global_config().verbose {
            logger::debug(format!(
                "Setting validator schedule:\n{validators_want:?}\n{leader_selection_want:?}"
            ));
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
            .num_validators()
            .call_raw()
            .block(block_id)
            .await
            .context("num_validators()")?
            .try_into()
            .ok()
            .context("num_validators() overflow")?;
        if global_config().verbose {
            logger::debug(format!(
                "Fetched number of validators from consensus registry: {n}"
            ));
        }

        multicall.block = Some(block_id);
        let validator_owners: Vec<Address> = multicall
            .add_calls(
                false,
                (0..n).map(|i| consensus_registry.validator_owners(i.into())),
            )
            .call_array()
            .await
            .context("validator_owners()")?;
        multicall.clear_calls();
        if global_config().verbose {
            logger::debug(format!(
                "Fetched validator owners from consensus registry: {validator_owners:?}"
            ));
        }

        let validators: Vec<abi::ValidatorsReturn> = multicall
            .add_calls(
                false,
                validator_owners
                    .iter()
                    .map(|addr| consensus_registry.validators(*addr)),
            )
            .call_array()
            .await
            .context("validators()")?;
        multicall.clear_calls();
        if global_config().verbose {
            logger::debug(format!(
                "Fetched validator info from consensus registry: {validators:?}"
            ));
        }

        // Update the validators.
        let mut txs = TxSet::default();
        // The final state that we want to set.
        let mut to_insert: HashMap<_, _> = validators_want
            .iter()
            .map(|a| (a.key.clone(), (a.pop.clone(), a.weight, a.leader)))
            .collect();

        // We'll go through each existing validator in the contract and see which changes need to be made.
        for (i, validator) in validators.into_iter().enumerate() {
            if validator.latest.removed {
                continue;
            }

            // Decode this validator info from the consensus registry.
            let validator_owner = validator_owners[i];
            let got = validator::ValidatorInfo {
                key: decode_validator_key(&validator.latest.pub_key)
                    .context("decode_validator_key()")?,
                weight: validator.latest.weight.into(),
                leader: validator.latest.leader,
            };

            if let Some((_, weight, leader)) = to_insert.remove(&got.key) {
                // If the weight has changed, we need to change it.
                if weight != got.weight {
                    txs.send(
                        format!(
                            "change_validator_weight({validator_owner:?}, {} -> {weight})",
                            got.weight
                        ),
                        consensus_registry.change_validator_weight(
                            validator_owners[i],
                            weight.try_into().context("weight overflow")?,
                        ),
                    )
                    .await?;
                }

                // If the leader status has changed, we need to change it.
                if leader != got.leader {
                    txs.send(
                        format!("change_validator_leader({validator_owner:?}, {leader})"),
                        consensus_registry.change_validator_leader(validator_owner, leader),
                    )
                    .await?;
                }

                // If the validator is not active, we need to activate it.
                if !validator.latest.active {
                    txs.send(
                        format!("activate({validator_owner:?})"),
                        consensus_registry.change_validator_active(validator_owner, true),
                    )
                    .await?;
                }

                // We don't change keys because in this case we are using them as an identifier and
                // the validator owner is just random. So to change the key we need to remove the
                // validator and add it back with the new key.
            } else {
                // If the validator is not in the final state, we need to remove it.
                txs.send(
                    format!("remove({validator_owner:?})"),
                    consensus_registry.remove(validator_owner),
                )
                .await?;
            }
        }

        for (key, (pop, weight, leader)) in to_insert {
            let owner = Address::random();
            txs.send(
                format!("add({key:?}, {weight})"),
                consensus_registry.add(
                    owner,
                    leader,
                    weight.try_into().context("overflow")?,
                    encode_validator_key(&key),
                    encode_validator_pop(&pop),
                ),
            )
            .await?;
        }

        // Update the leader selection.
        txs.send(
            "change_leader_selection".to_owned(),
            consensus_registry.update_leader_selection(
                leader_selection_want.frequency,
                leader_selection_want.weighted,
            ),
        )
        .await?;

        // Commit the validator schedule.
        txs.send(
            "commit_validator_committee".to_owned(),
            consensus_registry.commit_validator_committee(),
        )
        .await?;
        txs.wait(&provider).await.context("wait()")?;

        Ok(())
    }

    async fn set_schedule_activation_delay(&self, delay: u64) -> anyhow::Result<()> {
        if global_config().verbose {
            logger::debug(format!(
                "Setting committee activation delay to {delay} blocks"
            ));
        }

        let provider = self.provider().context("provider()")?;

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

        let owner = consensus_registry.owner().call().await.context("owner()")?;
        if owner != governor.address {
            anyhow::bail!(
                "governor ({:#x}) is different than the consensus registry owner ({:#x})",
                governor.address,
                owner
            );
        }

        // Send the transaction to set committee activation delay
        let mut txs = TxSet::default();
        txs.send(
            format!("set_committee_activation_delay({delay})"),
            consensus_registry.set_committee_activation_delay(delay.into()),
        )
        .await?;

        txs.wait(&provider).await.context("wait()")?;

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

fn decode_committee_validator(
    v: &abi::CommitteeValidator,
) -> anyhow::Result<validator::ValidatorInfo> {
    Ok(validator::ValidatorInfo {
        key: decode_validator_key(&v.pub_key).context("key")?,
        weight: v.weight.into(),
        leader: v.leader,
    })
}

fn decode_leader_selection(
    l: &abi::LeaderSelectionAttr,
) -> anyhow::Result<validator::LeaderSelection> {
    Ok(validator::LeaderSelection {
        frequency: l.frequency,
        mode: if l.weighted {
            validator::LeaderSelectionMode::Weighted
        } else {
            validator::LeaderSelectionMode::RoundRobin
        },
    })
}

fn print_schedule(schedule: &validator::Schedule, is_pending: bool) {
    let status = if is_pending { "Pending" } else { "Current" };
    let mut validators_info = format!("{} Validator Schedule:\nValidators:", status);

    for (i, validator) in schedule.iter().enumerate() {
        validators_info.push_str(&format!(
            "\n  Validator {}:\n    Key: {:?}\n    Weight: {}\n    Leader: {}",
            i + 1,
            validator.key,
            validator.weight,
            validator.leader
        ));
    }

    let leader_selection = schedule.leader_selection();
    let mode_str = match leader_selection.mode {
        validator::LeaderSelectionMode::Weighted => "Weighted",
        validator::LeaderSelectionMode::RoundRobin => "RoundRobin",
        _ => unreachable!(),
    };

    let leader_info = format!(
        "Leader Selection:\n  Frequency: {}\n  Mode: {}",
        leader_selection.frequency, mode_str
    );

    logger::success(format!("{}\n\n{}", validators_info, leader_info));
}
