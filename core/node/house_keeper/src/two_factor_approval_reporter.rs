use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_config::configs::eth_sender::SenderConfig;
use zksync_contracts::{era_multisig_validator_contract, hyperchain_contract};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::CallFunctionArgs;
use zksync_l1_contract_interface::i_executor::methods::ExecuteBatches;
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_types::{
    commitment::{L1BatchWithMetadata, PriorityOpsMerkleProof},
    hasher::keccak::KeccakHasher,
    l1::L1Tx,
    protocol_version::PACKED_SEMVER_MINOR_MASK,
    settlement::SettlementLayer,
    Address, ProtocolVersionId, H256, U256,
};
use zksync_web3_decl::client::{DynClient, L1};

use crate::{metrics::TWO_FACTOR_APPROVAL_METRICS, periodic_job::PeriodicJob};

/// Reports how long a committed batch has been waiting for enough 2FA approvals to be executed.
///
/// Some Era chains guard `executeBatchesSharedBridge` with `EraMultisigValidator`, an optional
/// wrapper deployed in front of `ValidatorTimelock` that requires a threshold of off-chain
/// approvers (see the contract's `approveHash`) to sign off on a batch execution before it can
/// proceed. If the approvers fall behind (or stop entirely), the real execute transaction keeps
/// reverting with `NotEnoughSignatures`.
///
/// The exact hash a batch needs approved can only be computed from the same data `eth_sender`
/// itself would submit, so this reporter mirrors `Aggregator::get_execute_operations` /
/// `EthTxAggregator::encode_aggregated_op` (`core/node/eth_sender/src/{aggregator,
/// eth_tx_aggregator}.rs`) closely - but deliberately does *not* wait for `eth_sender` to attempt
/// the real execute transaction, since that only happens once the batch's `executionDelay` (read
/// live from the `ValidatorTimelock`, and possibly close to an hour) has elapsed. Instead it
/// recomputes the same hash as soon as the batch is proven, using the shared, pure
/// `ExecuteBatches::encode_for_eth_tx` encoder rather than reimplementing the encoding itself.
/// If `eth_sender`'s encoding ever changes, this needs to be updated to match.
///
/// On chains where `validator_timelock_addr` is a plain `ValidatorTimelock` (no 2FA), the
/// `calculateHash` call below simply fails and this reporter leaves the metric at its default
/// value of 0 - no opt-in configuration is required.
#[derive(Debug)]
pub struct TwoFactorApprovalReporter {
    reporting_interval: Duration,
    connection_pool: ConnectionPool<Core>,
    eth_client: Box<DynClient<L1>>,
    validator_timelock_addr: Option<Address>,
    diamond_proxy_addr: Address,
    sender_config: SenderConfig,
}

impl TwoFactorApprovalReporter {
    pub fn new(
        reporting_interval: Duration,
        connection_pool: ConnectionPool<Core>,
        eth_client: Box<DynClient<L1>>,
        validator_timelock_addr: Option<Address>,
        diamond_proxy_addr: Address,
        sender_config: SenderConfig,
    ) -> Self {
        Self {
            reporting_interval,
            connection_pool,
            eth_client,
            validator_timelock_addr,
            diamond_proxy_addr,
            sender_config,
        }
    }

    /// Mirrors `EthTxAggregator::parse_protocol_version`.
    async fn get_chain_protocol_version(&self) -> anyhow::Result<ProtocolVersionId> {
        let contract = hyperchain_contract();
        let packed: U256 = CallFunctionArgs::new("getProtocolVersion", ())
            .for_contract(self.diamond_proxy_addr, &contract)
            .call(&self.eth_client)
            .await
            .context("getProtocolVersion call failed")?;
        if packed < U256::from(PACKED_SEMVER_MINOR_MASK) {
            ProtocolVersionId::try_from(packed.as_u32() as u16)
                .map_err(|_| anyhow::anyhow!("invalid protocol version id: {packed}"))
        } else {
            ProtocolVersionId::try_from_packed_semver(packed)
                .map_err(|_| anyhow::anyhow!("invalid packed protocol semver: {packed}"))
        }
    }

    /// Mirrors `eth_tx_aggregator::get_priority_tree_start_index`.
    async fn get_priority_tree_start_index(
        &self,
        chain_protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<Option<usize>> {
        if chain_protocol_version.is_pre_gateway() {
            return Ok(None);
        }
        let contract = hyperchain_contract();
        let index: U256 = CallFunctionArgs::new("getPriorityTreeStartIndex", ())
            .for_contract(self.diamond_proxy_addr, &contract)
            .call(&self.eth_client)
            .await
            .context("getPriorityTreeStartIndex call failed")?;
        Ok(Some(index.as_usize()))
    }

    /// Builds the `ExecuteBatches` payload for a single batch. Mirrors the relevant part of
    /// `Aggregator::get_execute_operations`, minus the `execution_delay` readiness gate.
    async fn build_execute_batches(
        conn: &mut Connection<'_, Core>,
        batch: L1BatchWithMetadata,
        priority_tree_start_index: usize,
        is_gateway: bool,
    ) -> anyhow::Result<ExecuteBatches> {
        let last_executed_priority_op_id = conn
            .blocks_dal()
            .get_last_executed_priority_op_id()
            .await
            .context("get_last_executed_priority_op_id")?
            .unwrap_or(priority_tree_start_index);

        let priority_op_hashes = conn
            .transactions_dal()
            .get_l1_transactions_hashes(priority_tree_start_index, last_executed_priority_op_id)
            .await
            .context("get_l1_transactions_hashes")?;
        let mut priority_merkle_tree =
            MiniMerkleTree::<L1Tx>::from_hashes(KeccakHasher, priority_op_hashes.into_iter(), None);

        let priority_ops_in_batch = conn
            .blocks_dal()
            .get_batch_first_and_last_priority_op_id(batch.header.number)
            .await
            .context("get_batch_first_and_last_priority_op_id")?
            .filter(|(first_id, _last_id)| *first_id >= priority_tree_start_index);

        let priority_ops_proof = if let Some((first_priority_op_id, last_priority_op_id)) =
            priority_ops_in_batch
        {
            let count = batch.header.l1_tx_count as usize;
            let new_hashes = conn
                .transactions_dal()
                .get_l1_transactions_hashes(
                    priority_tree_start_index + priority_merkle_tree.length(),
                    last_priority_op_id,
                )
                .await
                .context("get_l1_transactions_hashes (new)")?;
            for hash in new_hashes {
                priority_merkle_tree.push_hash(hash);
            }
            priority_merkle_tree.trim_start(
                first_priority_op_id
                    - priority_tree_start_index
                    - priority_merkle_tree.start_index(),
            );
            let (_, left, right) = priority_merkle_tree.merkle_root_and_paths_for_range(..count);
            let hashes = priority_merkle_tree.hashes_prefix(count);
            PriorityOpsMerkleProof {
                left_path: left.into_iter().map(Option::unwrap_or_default).collect(),
                right_path: right.into_iter().map(Option::unwrap_or_default).collect(),
                hashes,
            }
        } else {
            PriorityOpsMerkleProof::default()
        };

        let dependency_roots = conn
            .interop_root_dal()
            .get_interop_roots_batch(batch.header.number)
            .await
            .context("get_interop_roots_batch")?;

        let (logs, messages, message_roots) = if is_gateway {
            let message_root = batch
                .metadata
                .aggregation_root
                .context("missing aggregation_root for a gateway batch")?;
            (
                vec![batch.header.l2_to_l1_logs.clone()],
                vec![batch.header.l2_to_l1_messages.clone()],
                vec![message_root],
            )
        } else {
            (vec![], vec![], vec![])
        };

        Ok(ExecuteBatches {
            l1_batches: vec![batch],
            priority_ops_proofs: vec![priority_ops_proof],
            dependency_roots: vec![dependency_roots],
            logs,
            messages,
            message_roots,
        })
    }

    async fn report_metrics(&self) -> anyhow::Result<()> {
        let Some(validator_timelock_addr) = self.validator_timelock_addr else {
            return Ok(());
        };

        let mut conn = self
            .connection_pool
            .connection_tagged("house_keeper")
            .await?;

        // Delay-independent: only requires the batch to be proven, unlike what `eth_sender`
        // itself waits for before attempting the real execute transaction.
        let batch = conn
            .blocks_dal()
            .get_ready_for_execute_l1_batches(1, None, self.sender_config.prover)
            .await
            .context("get_ready_for_execute_l1_batches")?
            .into_iter()
            .next();
        let Some(batch) = batch else {
            // Nothing is waiting to be executed right now.
            TWO_FACTOR_APPROVAL_METRICS
                .committed_batch_2fa_approval_pending_seconds
                .set(0);
            return Ok(());
        };
        let l1_batch_number = batch.header.number;

        let Some(commit_confirmed_at) = conn
            .eth_sender_dal()
            .get_commit_confirmed_at(l1_batch_number)
            .await
            .context("get_commit_confirmed_at")?
        else {
            // Proven-but-not-yet-committed-confirmed shouldn't normally happen; skip this tick.
            return Ok(());
        };

        let is_gateway = conn
            .blocks_dal()
            .get_latest_sealed_l1_batch_header()
            .await
            .context("get_latest_sealed_l1_batch_header")?
            .map(|header| header.settlement_layer)
            .map(SettlementLayer::is_gateway)
            .unwrap_or(false);

        let chain_protocol_version = self
            .get_chain_protocol_version()
            .await
            .context("get_chain_protocol_version")?;
        let Some(priority_tree_start_index) = self
            .get_priority_tree_start_index(chain_protocol_version)
            .await
            .context("get_priority_tree_start_index")?
        else {
            // Pre-gateway chains don't support this readout; EraMultisigValidator requires a much
            // newer protocol version anyway, so this isn't expected in practice.
            return Ok(());
        };

        let execute_batches =
            Self::build_execute_batches(&mut conn, batch, priority_tree_start_index, is_gateway)
                .await
                .context("build_execute_batches")?;
        drop(conn);

        let settlement_fee_payer = self
            .sender_config
            .settlement_fee_payer
            .unwrap_or(Address::zero());
        let mut args =
            execute_batches.encode_for_eth_tx(chain_protocol_version, settlement_fee_payer);
        anyhow::ensure!(
            args.len() == 3,
            "unexpected number of tokens from encode_for_eth_tx for a single batch: {}",
            args.len()
        );
        let batch_data = args
            .remove(2)
            .into_bytes()
            .context("batchData token is not bytes")?;
        let process_batch_to = args
            .remove(1)
            .into_uint()
            .context("processBatchTo token is not a uint")?;
        let process_batch_from = args
            .remove(0)
            .into_uint()
            .context("processBatchFrom token is not a uint")?;

        // A plain `ValidatorTimelock` (no 2FA) doesn't implement `calculateHash`; this call
        // failing is the expected, common case for chains that don't use `EraMultisigValidator`.
        let contract = era_multisig_validator_contract();
        let hash: H256 = CallFunctionArgs::new(
            "calculateHash",
            (
                self.diamond_proxy_addr,
                process_batch_from,
                process_batch_to,
                batch_data,
            ),
        )
        .for_contract(validator_timelock_addr, &contract)
        .call(&self.eth_client)
        .await
        .context("calculateHash call failed (chain may not use EraMultisigValidator)")?;

        let approvals: U256 = CallFunctionArgs::new("getApprovals", hash)
            .for_contract(validator_timelock_addr, &contract)
            .call(&self.eth_client)
            .await
            .context("getApprovals call failed")?;
        let threshold: U256 = CallFunctionArgs::new("threshold", ())
            .for_contract(validator_timelock_addr, &contract)
            .call(&self.eth_client)
            .await
            .context("threshold call failed")?;

        let pending_seconds = if approvals < threshold {
            let now_secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Incorrect system time")
                .as_secs();
            let commit_confirmed_at_secs = commit_confirmed_at.and_utc().timestamp().max(0) as u64;
            now_secs.saturating_sub(commit_confirmed_at_secs)
        } else {
            0
        };
        TWO_FACTOR_APPROVAL_METRICS
            .committed_batch_2fa_approval_pending_seconds
            .set(pending_seconds);
        Ok(())
    }
}

#[async_trait]
impl PeriodicJob for TwoFactorApprovalReporter {
    const SERVICE_NAME: &'static str = "TwoFactorApprovalReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        // This is a monitoring task: an L1 RPC hiccup or an unexpected calldata shape must never
        // take down the rest of house_keeper (or the node), so errors are logged, not propagated.
        if let Err(err) = self.report_metrics().await {
            tracing::warn!("Failed to report 2FA approval metrics: {err:#}");
        }
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval.as_millis() as u64
    }
}
