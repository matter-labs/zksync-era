use std::{ops, time::Instant};

use anyhow::Context;
use zksync_dal::CoreDal;
use zksync_multivm::{
    interface::{ExecutionResult, TransactionExecutionMetrics},
    utils::{
        adjust_pubdata_price_for_tx, derive_base_fee_and_gas_per_pubdata, derive_overhead,
        get_max_batch_gas_limit,
    },
};
use zksync_system_constants::MAX_L2_TX_GAS_LIMIT;
use zksync_types::{
    api::state_override::StateOverride, fee::Fee, fee_model::BatchFeeInput, get_code_key,
    ExecuteTransactionCommon, PackedEthSignature, ProtocolVersionId, Transaction, H256,
};

use super::{result::ApiCallResult, SubmitTxError, TxSender};
use crate::execution_sandbox::{BlockArgs, SandboxAction, VmPermit, SANDBOX_METRICS};

#[derive(Debug, Clone, Copy)]
pub(crate) enum BinarySearchKind {
    /// Full binary search.
    Full,
    /// Binary search with an optimized initial pivot.
    Optimized,
}

impl BinarySearchKind {
    pub(crate) fn new(optimize: bool) -> Self {
        if optimize {
            Self::Optimized
        } else {
            Self::Full
        }
    }
}

impl TxSender {
    #[tracing::instrument(level = "debug", skip_all, fields(
        initiator = ?tx.initiator_account(),
        nonce = ?tx.nonce(),
    ))]
    pub async fn get_txs_fee_in_wei(
        &self,
        tx: Transaction,
        block_args: BlockArgs,
        estimated_fee_scale_factor: f64,
        acceptable_overestimation: u64,
        state_override: Option<StateOverride>,
        kind: BinarySearchKind,
    ) -> Result<Fee, SubmitTxError> {
        let estimation_started_at = Instant::now();
        let mut estimator = GasEstimator::new(self, tx, block_args, state_override).await?;
        estimator.adjust_transaction_fee();

        let initial_estimate = estimator.initialize().await?;
        tracing::trace!(
            "preparation took {:?}, starting binary search",
            estimation_started_at.elapsed()
        );

        let optimized_lower_bound = initial_estimate.lower_gas_bound_without_overhead();
        // Perform an initial search iteration with the pivot slightly greater than `gas_used` to account for 63/64 rule for far calls etc.
        // If the transaction succeeds, it will discard most of the search space at once.
        let optimistic_gas_limit = initial_estimate.optimistic_gas_limit_without_overhead();

        let (bounds, initial_pivot) = match kind {
            BinarySearchKind::Full => {
                let lower_bound = initial_estimate.gas_charged_for_pubdata;
                let upper_bound = MAX_L2_TX_GAS_LIMIT + initial_estimate.gas_charged_for_pubdata;
                (lower_bound..=upper_bound, None)
            }
            BinarySearchKind::Optimized => {
                let lower_bound =
                    optimized_lower_bound.unwrap_or(initial_estimate.gas_charged_for_pubdata);
                let upper_bound = MAX_L2_TX_GAS_LIMIT + initial_estimate.gas_charged_for_pubdata;
                let initial_pivot = optimistic_gas_limit.filter(|&gas| {
                    // If `optimistic_gas_limit` is greater than the ordinary binary search pivot, there's no sense using it.
                    gas < (lower_bound + upper_bound) / 2
                });
                (lower_bound..=upper_bound, initial_pivot)
            }
        };

        let (unscaled_gas_limit, iteration_count) =
            Self::binary_search(&estimator, bounds, initial_pivot, acceptable_overestimation)
                .await?;
        // Metrics are intentionally reported regardless of the binary search mode, so that the collected stats can be used to adjust
        // optimized binary search params (e.g., the initial pivot multiplier).
        if let Some(lower_bound) = optimized_lower_bound {
            let tx_overhead = estimator.tx_overhead(unscaled_gas_limit);
            let diff = (unscaled_gas_limit as f64 - lower_bound as f64)
                / (unscaled_gas_limit + tx_overhead) as f64;
            SANDBOX_METRICS
                .estimate_gas_lower_bound_relative_diff
                .observe(diff);
        }
        if let Some(optimistic_gas_limit) = optimistic_gas_limit {
            let tx_overhead = estimator.tx_overhead(unscaled_gas_limit);
            let diff = (optimistic_gas_limit as f64 - unscaled_gas_limit as f64)
                / (unscaled_gas_limit + tx_overhead) as f64;
            SANDBOX_METRICS
                .estimate_gas_optimistic_gas_limit_relative_diff
                .observe(diff);
        }
        tracing::debug!(
            optimized_lower_bound,
            optimistic_gas_limit,
            unscaled_gas_limit,
            binary_search = ?kind,
            iteration_count,
            "Finished estimating gas limit for transaction"
        );

        let suggested_gas_limit = (unscaled_gas_limit as f64 * estimated_fee_scale_factor) as u64;

        #[cfg(feature = "zkos")]
        // TODO: estimate gas should estimate validation cost, currently we just add some gas on top.
        let suggested_gas_limit = suggested_gas_limit + 10000;

        estimator
            .finalize(suggested_gas_limit, estimated_fee_scale_factor)
            .await
    }

    async fn binary_search(
        estimator: &GasEstimator<'_>,
        bounds: ops::RangeInclusive<u64>,
        initial_pivot: Option<u64>,
        acceptable_overestimation: u64,
    ) -> Result<(u64, usize), SubmitTxError> {
        let mut number_of_iterations = 0;
        let mut lower_bound = *bounds.start();
        let mut upper_bound = *bounds.end();

        if let Some(pivot) = initial_pivot {
            let iteration_started_at = Instant::now();
            let (result, _) = estimator.step(pivot).await?;
            Self::adjust_search_bounds(&mut lower_bound, &mut upper_bound, pivot, &result);

            tracing::trace!(
                "iteration {number_of_iterations} took {:?}. lower_bound: {lower_bound}, upper_bound: {upper_bound}",
                iteration_started_at.elapsed()
            );
            number_of_iterations += 1;
        }

        // We are using binary search to find the minimal values of gas_limit under which the transaction succeeds.
        while lower_bound + acceptable_overestimation < upper_bound {
            let mid = (lower_bound + upper_bound) / 2;
            // There is no way to distinct between errors due to out of gas
            // or normal execution errors, so we just hope that increasing the
            // gas limit will make the transaction successful
            let iteration_started_at = Instant::now();
            let (result, _) = estimator.step(mid).await?;
            Self::adjust_search_bounds(&mut lower_bound, &mut upper_bound, mid, &result);

            tracing::trace!(
                "iteration {number_of_iterations} took {:?}. lower_bound: {lower_bound}, upper_bound: {upper_bound}",
                iteration_started_at.elapsed()
            );
            number_of_iterations += 1;
        }
        SANDBOX_METRICS
            .estimate_gas_binary_search_iterations
            .observe(number_of_iterations);
        Ok((upper_bound, number_of_iterations))
    }

    async fn ensure_sufficient_balance(
        &self,
        tx: &Transaction,
        state_override: Option<&StateOverride>,
    ) -> Result<(), SubmitTxError> {
        let hashed_key = get_code_key(&tx.initiator_account());
        // If the default account does not have enough funds for transferring `tx.value`, without taking into account the fee,
        // there is no sense to estimate the fee.
        let account_code_hash = self
            .acquire_replica_connection()
            .await?
            .storage_web3_dal()
            .get_value(&hashed_key)
            .await
            .with_context(|| {
                format!(
                    "failed getting code hash for account {:?}",
                    tx.initiator_account()
                )
            })?;

        if !tx.is_l1() && account_code_hash == H256::zero() {
            let balance = match state_override
                .and_then(|overrides| overrides.get(&tx.initiator_account()))
                .and_then(|account| account.balance)
            {
                Some(balance) => balance,
                None => self.get_balance(&tx.initiator_account()).await?,
            };

            if tx.execute.value > balance {
                tracing::info!(
                    "fee estimation failed on validation step.
                    account: {} does not have enough funds for for transferring tx.value: {}.",
                    tx.initiator_account(),
                    tx.execute.value
                );
                return Err(SubmitTxError::NotEnoughBalanceForFeeValue(
                    balance,
                    0.into(),
                    tx.execute.value,
                ));
            }
        }
        Ok(())
    }

    fn adjust_search_bounds(
        lower_bound: &mut u64,
        upper_bound: &mut u64,
        pivot: u64,
        result: &ExecutionResult,
    ) {
        // For now, we don't discern between "out of gas" and other failure reasons since it's difficult in the general case.
        if result.is_failed() {
            *lower_bound = pivot + 1;
        } else {
            *upper_bound = pivot;
        }
    }
}

/// Initial gas estimate with effectively infinite gas limit.
#[derive(Debug)]
pub(super) struct InitialGasEstimate {
    /// Set to `None` if not estimated (e.g., for L1 transactions), or if the VM returned bogus refund stats.
    pub total_gas_charged: Option<u64>,
    /// Set to `None` if not estimated (e.g., for L1 transactions).
    pub computational_gas_used: Option<u64>,
    /// Operator-defined overhead for the estimated transaction. For recent VM versions, the overhead only depends
    /// on the transaction encoding size.
    pub operator_overhead: u64,
    pub gas_charged_for_pubdata: u64,
}

impl InitialGasEstimate {
    /// Returns the lower gas limit bound, i.e., gas limit that is guaranteed to be lower than the minimum passing gas limit,
    /// but is reasonably close to it.
    ///
    /// # Background
    ///
    /// Total gas charged for a transaction consists of:
    ///
    /// - Operator-set overhead (`self.operator_overhead`)
    /// - Intrinsic bootloader overhead
    /// - Gas used during validation / execution (`self.computational_gas_used`)
    /// - Gas charged for pubdata at the end of execution (`self.gas_charged_for_pubdata`)
    ///
    /// We add `operator_overhead` manually to the binary search argument at each `step()` because it depends on the gas limit in the general case,
    /// so the returned value corresponds to the other 3 terms.
    ///
    /// If the value cannot be computed, it is set to `None`.
    pub fn lower_gas_bound_without_overhead(&self) -> Option<u64> {
        // The two ways to compute the used gas (by `computational_gas_used` and by the charged gas) don't return the identical values
        // due to various factors:
        //
        // - `computational_gas_used` tracks gas usage for the entire VM execution, while the transaction initiator (or a paymaster) is only charged
        //   for a part of it.
        // - The bootloader is somewhat lenient in the case pubdata costs are approximately equal to the amount of gas left
        //   (i.e., for some transaction types, such as base token transfers, there exists an entire range of gas limit values
        //   which all lead to a successful execution with 0 refund).
        //
        // We use the lesser of these two estimates as the lower bound.
        let mut total_gas_bound = self.computational_gas_used? + self.gas_charged_for_pubdata;
        if let Some(gas_charged) = self.total_gas_charged {
            total_gas_bound = total_gas_bound.min(gas_charged);
        }
        total_gas_bound.checked_sub(self.operator_overhead)
    }

    /// Returns heuristically chosen gas limit without operator overhead that should be sufficient for most transactions.
    /// This value is reasonably close to the lower gas limit bound, so that when used as the initial binary search pivot,
    /// it will discard most of the search space in the average case.
    pub fn optimistic_gas_limit_without_overhead(&self) -> Option<u64> {
        let gas_charged_without_overhead = self
            .total_gas_charged?
            .checked_sub(self.operator_overhead)?;
        // 21/20 is an empirical multiplier. It is higher than what empirically suffices for some common transactions;
        // one can argue that using 64/63 multiplier would be more accurate due to the 63/64 rule for far calls.
        // However, far calls are not the only source of gas overhead in Era; another one are decommit operations.
        Some(gas_charged_without_overhead * 21 / 20)
    }
}

/// Encapsulates gas estimation process for a specific transaction.
///
/// Public for testing purposes.
#[derive(Debug)]
pub(super) struct GasEstimator<'a> {
    sender: &'a TxSender,
    transaction: Transaction,
    state_override: Option<StateOverride>,
    vm_permit: VmPermit,
    fee_input: BatchFeeInput,
    base_fee: u64,
    gas_per_pubdata_byte: u64,
    max_gas_limit: u64,
    block_args: BlockArgs,
    protocol_version: ProtocolVersionId,
}

impl<'a> GasEstimator<'a> {
    pub(super) async fn new(
        sender: &'a TxSender,
        mut transaction: Transaction,
        block_args: BlockArgs,
        state_override: Option<StateOverride>,
    ) -> Result<Self, SubmitTxError> {
        let protocol_version = block_args.protocol_version();

        #[cfg(feature = "zkos")]
        let max_gas_limit = 100_000_000;

        #[cfg(not(feature = "zkos"))]
        let max_gas_limit = get_max_batch_gas_limit(protocol_version.into());

        let fee_input = adjust_pubdata_price_for_tx(
            sender.scaled_batch_fee_input().await?,
            transaction.gas_per_pubdata_byte_limit(),
            // We do not have to adjust the params to the `gasPrice` of the transaction, since
            // its gas price will be amended later on to suit the `fee_input`
            None,
            protocol_version.into(),
        );
        let (base_fee, gas_per_pubdata_byte) =
            derive_base_fee_and_gas_per_pubdata(fee_input, protocol_version.into());

        sender
            .ensure_sufficient_balance(&transaction, state_override.as_ref())
            .await?;

        // For L2 transactions we need a properly formatted signature
        if let ExecuteTransactionCommon::L2(l2_common_data) = &mut transaction.common_data {
            if l2_common_data.signature.is_empty() {
                l2_common_data.signature = PackedEthSignature::default().serialize_packed().into();
            }
        }

        // Acquire the vm token for the whole duration of the binary search.
        let vm_permit = sender.0.vm_concurrency_limiter.acquire().await;
        let vm_permit = vm_permit.ok_or(SubmitTxError::ServerShuttingDown)?;

        Ok(Self {
            sender,
            transaction,
            state_override,
            vm_permit,
            fee_input,
            base_fee,
            gas_per_pubdata_byte,
            max_gas_limit,
            block_args,
            protocol_version,
        })
    }

    pub(super) fn adjust_transaction_fee(&mut self) {
        match &mut self.transaction.common_data {
            ExecuteTransactionCommon::L2(common_data) => {
                common_data.fee.max_fee_per_gas = self.base_fee.into();
                common_data.fee.max_priority_fee_per_gas = self.base_fee.into();
            }
            ExecuteTransactionCommon::L1(common_data) => {
                common_data.max_fee_per_gas = self.base_fee.into();
            }
            ExecuteTransactionCommon::ProtocolUpgrade(common_data) => {
                common_data.max_fee_per_gas = self.base_fee.into();
            }
        }
    }

    pub(super) async fn initialize(&self) -> Result<InitialGasEstimate, SubmitTxError> {
        let operator_overhead = self.tx_overhead(self.max_gas_limit);

        // When the pubdata cost grows very high, the total gas limit required may become very high as well. If
        // we do binary search over any possible gas limit naively, we may end up with a very high number of iterations,
        // which affects performance.
        //
        // To optimize for this case, we first calculate the amount of gas needed to cover for the pubdata. After that, we
        // need to do a smaller binary search that is focused on computational gas limit only.
        if self.transaction.is_l1() {
            // For L1 transactions the pubdata priced in such a way that the maximal computational
            // gas limit should be enough to cover for the pubdata as well, so no additional gas is provided there.
            Ok(InitialGasEstimate {
                total_gas_charged: None,
                computational_gas_used: None,
                operator_overhead,
                gas_charged_for_pubdata: 0,
            })
        } else {
            // For L2 transactions, we estimate the amount of gas needed to cover for the pubdata by creating a transaction with infinite gas limit,
            // and getting how much pubdata it used.

            let (result, metrics) = self.unadjusted_step(self.max_gas_limit).await?;
            // If the transaction has failed with such a large gas limit, we return an API error here right away,
            // since the inferred gas bounds would be unreliable in this case.
            result.check_api_call_result()?;

            // It is assumed that there is no overflow here
            let gas_charged_for_pubdata =
                u64::from(metrics.vm.pubdata_published) * self.gas_per_pubdata_byte;

            let total_gas_charged = self.max_gas_limit.checked_sub(metrics.gas_refunded);
            Ok(InitialGasEstimate {
                total_gas_charged,
                computational_gas_used: Some(metrics.vm.computational_gas_used.into()),
                operator_overhead,
                gas_charged_for_pubdata,
            })
        }
    }

    /// Derives operator overhead for a transaction given its gas limit.
    fn tx_overhead(&self, tx_gas_limit: u64) -> u64 {
        derive_overhead(
            tx_gas_limit,
            self.gas_per_pubdata_byte as u32,
            self.transaction.encoding_len(),
            self.transaction.tx_format() as u8,
            self.protocol_version.into(),
        )
        .into()
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn step(
        &self,
        tx_gas_limit: u64,
    ) -> Result<(ExecutionResult, TransactionExecutionMetrics), SubmitTxError> {
        let gas_limit_with_overhead = tx_gas_limit + self.tx_overhead(tx_gas_limit);
        // We need to ensure that we never use a gas limit that is higher than the maximum allowed
        let forced_gas_limit =
            gas_limit_with_overhead.min(get_max_batch_gas_limit(self.protocol_version.into()));
        self.unadjusted_step(forced_gas_limit).await
    }

    pub(super) async fn unadjusted_step(
        &self,
        forced_gas_limit: u64,
    ) -> Result<(ExecutionResult, TransactionExecutionMetrics), SubmitTxError> {
        let mut tx = self.transaction.clone();
        match &mut tx.common_data {
            ExecuteTransactionCommon::L1(l1_common_data) => {
                l1_common_data.gas_limit = forced_gas_limit.into();
                // Since `tx.execute.value` is supplied by the client and is not checked against the current balance (unlike for L2 transactions),
                // we may hit an integer overflow. Ditto for protocol upgrade transactions below.
                let required_funds = (l1_common_data.gas_limit * l1_common_data.max_fee_per_gas)
                    .checked_add(tx.execute.value)
                    .ok_or(SubmitTxError::MintedAmountOverflow)?;
                l1_common_data.to_mint = required_funds;
            }
            ExecuteTransactionCommon::L2(l2_common_data) => {
                l2_common_data.fee.gas_limit = forced_gas_limit.into();
            }
            ExecuteTransactionCommon::ProtocolUpgrade(common_data) => {
                common_data.gas_limit = forced_gas_limit.into();
                let required_funds = (common_data.gas_limit * common_data.max_fee_per_gas)
                    .checked_add(tx.execute.value)
                    .ok_or(SubmitTxError::MintedAmountOverflow)?;
                common_data.to_mint = required_funds;
            }
        }

        let action = SandboxAction::GasEstimation {
            tx,
            fee_input: self.fee_input,
            base_fee: self.base_fee,
        };
        let connection = self.sender.acquire_replica_connection().await?;
        let executor = &self.sender.0.executor;
        let execution_output = executor
            .execute_in_sandbox(
                self.vm_permit.clone(),
                connection,
                action,
                &self.block_args,
                self.state_override.clone(),
            )
            .await?;
        Ok((execution_output.result, execution_output.metrics))
    }

    async fn finalize(
        self,
        suggested_gas_limit: u64,
        estimated_fee_scale_factor: f64,
    ) -> Result<Fee, SubmitTxError> {
        let (result, tx_metrics) = self.step(suggested_gas_limit).await?;
        result.into_api_call_result()?;
        self.sender
            .ensure_tx_executable(&self.transaction, tx_metrics, false)?;

        // Now, we need to calculate the final overhead for the transaction.
        let overhead = derive_overhead(
            suggested_gas_limit,
            self.gas_per_pubdata_byte as u32,
            self.transaction.encoding_len(),
            self.transaction.tx_format() as u8,
            self.protocol_version.into(),
        );

        let full_gas_limit = match suggested_gas_limit.overflowing_add(overhead.into()) {
            (value, false) => {
                if value > self.max_gas_limit {
                    return Err(SubmitTxError::ExecutionReverted(
                        "exceeds block gas limit".to_string(),
                        vec![],
                    ));
                }

                value
            }
            (_, true) => {
                return Err(SubmitTxError::ExecutionReverted(
                    "exceeds block gas limit".to_string(),
                    vec![],
                ));
            }
        };

        let gas_for_pubdata =
            u64::from(tx_metrics.vm.pubdata_published) * self.gas_per_pubdata_byte;
        let estimated_gas_for_pubdata =
            (gas_for_pubdata as f64 * estimated_fee_scale_factor) as u64;

        tracing::debug!(
            "gas for pubdata: {estimated_gas_for_pubdata}, computational gas: {comp_gas}, overhead gas: {overhead} \
            (with params base_fee: {base_fee}, gas_per_pubdata_byte: {gas_per_pubdata_byte}) \
            estimated_fee_scale_factor: {estimated_fee_scale_factor}",
            comp_gas = suggested_gas_limit - estimated_gas_for_pubdata,
            base_fee = self.base_fee,
            gas_per_pubdata_byte = self.gas_per_pubdata_byte
        );

        // Given that we scale overall fee, we should also scale the limit for gas per pubdata price that user agrees to.
        // However, we should not exceed the limit that was provided by the user in the initial request.
        let gas_per_pubdata_limit = std::cmp::min(
            ((self.gas_per_pubdata_byte as f64 * estimated_fee_scale_factor) as u64).into(),
            self.transaction.gas_per_pubdata_byte_limit(),
        );

        Ok(Fee {
            max_fee_per_gas: self.base_fee.into(),
            max_priority_fee_per_gas: 0u32.into(),
            gas_limit: full_gas_limit.into(),
            gas_per_pubdata_limit,
        })
    }
}
