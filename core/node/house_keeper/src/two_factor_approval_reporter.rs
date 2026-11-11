use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_config::configs::eth_sender::SenderConfig;
use zksync_contracts::{era_multisig_validator_contract, hyperchain_contract};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::CallFunctionArgs;
use zksync_eth_sender::build_execute_batches_payload;
use zksync_types::{
    protocol_version::PACKED_SEMVER_MINOR_MASK, settlement::SettlementLayer, Address,
    ProtocolVersionId, H256, U256,
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
/// itself would submit, so this reuses `eth_sender`'s own `build_execute_batches_payload`
/// (`core/node/eth_sender/src/aggregator.rs`) - the same pure function `Aggregator` calls - to
/// avoid maintaining a second, drift-prone copy of that encoding. Unlike `Aggregator`, this
/// deliberately does *not* wait for the batch's `executionDelay` (read live from
/// `ValidatorTimelock`, and possibly close to an hour) to elapse, nor for it to be proven: a
/// batch's commitment/metadata - and so the hash it needs approved - is fully determined as soon
/// as it's committed (proof doesn't add data `calculateHash` needs, it's only a prerequisite for
/// the real `executeBatchesSharedBridge` call to succeed). So this recomputes the hash right away
/// at commit time, independent of when `eth_sender` actually attempts to execute the batch.
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

    async fn report_metrics(&self) -> anyhow::Result<()> {
        let Some(validator_timelock_addr) = self.validator_timelock_addr else {
            return Ok(());
        };

        let mut conn = self
            .connection_pool
            .connection_tagged("house_keeper")
            .await?;

        // Delay- and proof-independent: keyed only on commit confirmation, unlike what
        // `eth_sender` itself waits for before attempting the real execute transaction.
        let Some((l1_batch_number, commit_confirmed_at)) = conn
            .eth_sender_dal()
            .get_oldest_committed_unexecuted_batch()
            .await
            .context("get_oldest_committed_unexecuted_batch")?
        else {
            // Nothing is waiting to be executed right now.
            TWO_FACTOR_APPROVAL_METRICS
                .committed_batch_2fa_approval_pending_seconds
                .set(0);
            return Ok(());
        };

        let Some(batch) = conn
            .blocks_dal()
            .get_l1_batch_metadata_with_prover(l1_batch_number, self.sender_config.prover)
            .await
            .context("get_l1_batch_metadata_with_prover")?
        else {
            // Committed-but-not-yet-metadata-calculated shouldn't normally happen (commit itself
            // requires metadata to build its calldata); skip this tick.
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

        // `&mut None`: a one-off build, not amortized across polls like `Aggregator` does.
        let execute_batches = build_execute_batches_payload(
            &mut conn,
            vec![batch],
            Some(priority_tree_start_index),
            &mut None,
            is_gateway,
        )
        .await
        .context("build_execute_batches_payload")?;
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
