use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_contracts::era_multisig_validator_contract;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::CallFunctionArgs;
use zksync_types::{Address, H256, U256};
use zksync_web3_decl::client::{DynClient, L1};

use crate::{metrics::TWO_FACTOR_APPROVAL_METRICS, periodic_job::PeriodicJob};

/// Reports how long a committed batch has been waiting for enough 2FA approvals to be executed.
///
/// Some Era chains guard `executeBatchesSharedBridge` with `EraMultisigValidator`, an optional
/// wrapper deployed in front of `ValidatorTimelock` that requires a threshold of off-chain
/// approvers (see the contract's `approveHash`) to sign off on a batch execution before it can
/// proceed. If the approvers fall behind (or stop entirely), the real execute transaction keeps
/// reverting with `NotEnoughSignatures`. This reporter is the earliest DB-visible signal of that:
/// it inspects the exact `executeBatchesSharedBridge` calldata `eth_sender` already built for the
/// oldest unconfirmed execute attempt, and asks the contract directly (`getApprovals`/`threshold`)
/// whether it has enough approvals yet.
///
/// On chains where `validator_timelock_addr` is a plain `ValidatorTimelock` (no 2FA), the extra
/// view calls simply fail and this reporter leaves the metric at its default value of 0 - no
/// opt-in configuration is required.
#[derive(Debug)]
pub struct TwoFactorApprovalReporter {
    reporting_interval: Duration,
    connection_pool: ConnectionPool<Core>,
    eth_client: Box<DynClient<L1>>,
    validator_timelock_addr: Option<Address>,
}

impl TwoFactorApprovalReporter {
    pub fn new(
        reporting_interval: Duration,
        connection_pool: ConnectionPool<Core>,
        eth_client: Box<DynClient<L1>>,
        validator_timelock_addr: Option<Address>,
    ) -> Self {
        Self {
            reporting_interval,
            connection_pool,
            eth_client,
            validator_timelock_addr,
        }
    }

    async fn report_metrics(&self) -> anyhow::Result<()> {
        let Some(validator_timelock_addr) = self.validator_timelock_addr else {
            return Ok(());
        };

        let mut conn = self
            .connection_pool
            .connection_tagged("house_keeper")
            .await?;
        let pending = conn
            .eth_sender_dal()
            .get_oldest_pending_execute_tx()
            .await?;
        drop(conn);

        let Some(pending) = pending else {
            // Nothing is waiting to be executed right now.
            TWO_FACTOR_APPROVAL_METRICS
                .committed_batch_2fa_approval_pending_seconds
                .set(0);
            return Ok(());
        };

        let contract = era_multisig_validator_contract();
        let execute_function = contract
            .function("executeBatchesSharedBridge")
            .context("executeBatchesSharedBridge not found in EraMultisigValidator ABI")?;

        anyhow::ensure!(
            pending.execute_tx_raw.len() >= 4,
            "execute tx raw calldata for batch #{} is shorter than a function selector",
            pending.l1_batch_number
        );
        let tokens = execute_function
            .decode_input(&pending.execute_tx_raw[4..])
            .with_context(|| {
                format!(
                    "failed decoding execute tx calldata for batch #{}",
                    pending.l1_batch_number
                )
            })?;
        let [chain_address, process_batch_from, process_batch_to, batch_data]: [_; 4] =
            tokens.try_into().map_err(|tokens: Vec<_>| {
                anyhow::anyhow!(
                    "unexpected number of decoded execute tx params: {}",
                    tokens.len()
                )
            })?;
        let chain_address = chain_address
            .into_address()
            .context("chainAddress is not an address")?;
        let process_batch_from = process_batch_from
            .into_uint()
            .context("processBatchFrom is not a uint")?;
        let process_batch_to = process_batch_to
            .into_uint()
            .context("processBatchTo is not a uint")?;
        let batch_data = batch_data.into_bytes().context("batchData is not bytes")?;

        // A plain `ValidatorTimelock` (no 2FA) doesn't implement `calculateHash`; this call
        // failing is the expected, common case for chains that don't use `EraMultisigValidator`.
        let hash: H256 = CallFunctionArgs::new(
            "calculateHash",
            (
                chain_address,
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
            let commit_confirmed_at_secs =
                pending.commit_confirmed_at.and_utc().timestamp().max(0) as u64;
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
