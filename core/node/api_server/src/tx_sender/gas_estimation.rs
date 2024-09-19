use std::time::Instant;

use anyhow::Context;
use zksync_dal::CoreDal;
use zksync_multivm::{
    interface::{
        OneshotTracingParams, TransactionExecutionMetrics, TxExecutionArgs, TxExecutionMode,
        VmExecutionResultAndLogs,
    },
    utils::{
        adjust_pubdata_price_for_tx, derive_base_fee_and_gas_per_pubdata, derive_overhead,
        get_max_batch_gas_limit,
    },
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
    zk_evm_latest::ethereum_types::H256,
    VmVersion,
};
use zksync_system_constants::MAX_L2_TX_GAS_LIMIT;
use zksync_types::{
    api::state_override::StateOverride, fee::Fee, fee_model::BatchFeeInput, get_code_key,
    AccountTreeId, ExecuteTransactionCommon, PackedEthSignature, Transaction,
};

use super::{result::ApiCallResult, SubmitTxError, TxSender};
use crate::execution_sandbox::{BlockArgs, TxSetupArgs, VmPermit, SANDBOX_METRICS};

impl TxSender {
    /// Given the gas_limit to be used for the body of the transaction,
    /// returns the result for executing the transaction with such gas_limit
    #[allow(clippy::too_many_arguments)]
    async fn estimate_gas_step(
        &self,
        vm_permit: VmPermit,
        mut tx: Transaction,
        tx_gas_limit: u64,
        gas_price_per_pubdata: u32,
        fee_model_params: BatchFeeInput,
        block_args: BlockArgs,
        base_fee: u64,
        vm_version: VmVersion,
        state_override: Option<StateOverride>,
    ) -> anyhow::Result<(VmExecutionResultAndLogs, TransactionExecutionMetrics)> {
        let overhead = derive_overhead(
            tx_gas_limit,
            gas_price_per_pubdata,
            tx.encoding_len(),
            tx.tx_format() as u8,
            vm_version,
        );
        let gas_limit_with_overhead = tx_gas_limit + overhead as u64;
        // We need to ensure that we never use a gas limit that is higher than the maximum allowed
        let forced_gas_limit = gas_limit_with_overhead.min(get_max_batch_gas_limit(vm_version));

        match &mut tx.common_data {
            ExecuteTransactionCommon::L1(l1_common_data) => {
                l1_common_data.gas_limit = forced_gas_limit.into();
                let required_funds =
                    l1_common_data.gas_limit * l1_common_data.max_fee_per_gas + tx.execute.value;
                l1_common_data.to_mint = required_funds;
            }
            ExecuteTransactionCommon::L2(l2_common_data) => {
                l2_common_data.fee.gas_limit = forced_gas_limit.into();
            }
            ExecuteTransactionCommon::ProtocolUpgrade(common_data) => {
                common_data.gas_limit = forced_gas_limit.into();
                let required_funds =
                    common_data.gas_limit * common_data.max_fee_per_gas + tx.execute.value;
                common_data.to_mint = required_funds;
            }
        }

        let setup_args = self.args_for_gas_estimate(fee_model_params, base_fee).await;
        let execution_args = TxExecutionArgs::for_gas_estimate(tx);
        let connection = self.acquire_replica_connection().await?;
        let execution_output = self
            .0
            .executor
            .execute_tx_in_sandbox(
                vm_permit,
                setup_args,
                execution_args,
                connection,
                block_args,
                state_override,
                OneshotTracingParams::default(),
            )
            .await?;
        Ok((execution_output.vm, execution_output.metrics))
    }

    async fn args_for_gas_estimate(&self, fee_input: BatchFeeInput, base_fee: u64) -> TxSetupArgs {
        let config = &self.0.sender_config;
        TxSetupArgs {
            execution_mode: TxExecutionMode::EstimateFee,
            operator_account: AccountTreeId::new(config.fee_account_addr),
            fee_input,
            // We want to bypass the computation gas limit check for gas estimation
            validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
            base_system_contracts: self.0.api_contracts.estimate_gas.clone(),
            caches: self.storage_caches(),
            chain_id: config.chain_id,
            whitelisted_tokens_for_aa: self.read_whitelisted_tokens_for_aa_cache().await,
            enforced_base_fee: Some(base_fee),
        }
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        initiator = ?tx.initiator_account(),
        nonce = ?tx.nonce(),
    ))]
    pub async fn get_txs_fee_in_wei(
        &self,
        mut tx: Transaction,
        estimated_fee_scale_factor: f64,
        acceptable_overestimation: u64,
        state_override: Option<StateOverride>,
    ) -> Result<Fee, SubmitTxError> {
        let estimation_started_at = Instant::now();

        let mut connection = self.acquire_replica_connection().await?;
        let block_args = BlockArgs::pending(&mut connection).await?;
        let protocol_version = connection
            .blocks_dal()
            .pending_protocol_version()
            .await
            .context("failed getting pending protocol version")?;
        let max_gas_limit = get_max_batch_gas_limit(protocol_version.into());
        drop(connection);

        let fee_input = adjust_pubdata_price_for_tx(
            self.scaled_batch_fee_input().await?,
            tx.gas_per_pubdata_byte_limit(),
            // We do not have to adjust the params to the `gasPrice` of the transaction, since
            // its gas price will be amended later on to suit the `fee_input`
            None,
            protocol_version.into(),
        );

        let (base_fee, gas_per_pubdata_byte) =
            derive_base_fee_and_gas_per_pubdata(fee_input, protocol_version.into());
        match &mut tx.common_data {
            ExecuteTransactionCommon::L2(common_data) => {
                common_data.fee.max_fee_per_gas = base_fee.into();
                common_data.fee.max_priority_fee_per_gas = base_fee.into();
            }
            ExecuteTransactionCommon::L1(common_data) => {
                common_data.max_fee_per_gas = base_fee.into();
            }
            ExecuteTransactionCommon::ProtocolUpgrade(common_data) => {
                common_data.max_fee_per_gas = base_fee.into();
            }
        }

        self.ensure_sufficient_balance(&tx, state_override.as_ref())
            .await?;

        // For L2 transactions we need a properly formatted signature
        if let ExecuteTransactionCommon::L2(l2_common_data) = &mut tx.common_data {
            if l2_common_data.signature.is_empty() {
                l2_common_data.signature = PackedEthSignature::default().serialize_packed().into();
            }
        }

        // Acquire the vm token for the whole duration of the binary search.
        let vm_permit = self.0.vm_concurrency_limiter.acquire().await;
        let vm_permit = vm_permit.ok_or(SubmitTxError::ServerShuttingDown)?;

        // When the pubdata cost grows very high, the total gas limit required may become very high as well. If
        // we do binary search over any possible gas limit naively, we may end up with a very high number of iterations,
        // which affects performance.
        //
        // To optimize for this case, we first calculate the amount of gas needed to cover for the pubdata. After that, we
        // need to do a smaller binary search that is focused on computational gas limit only.
        let (gas_used, additional_gas_for_pubdata) = if tx.is_l1() {
            // For L1 transactions the pubdata priced in such a way that the maximal computational
            // gas limit should be enough to cover for the pubdata as well, so no additional gas is provided there.
            (None, 0_u64)
        } else {
            // For L2 transactions, we estimate the amount of gas needed to cover for the pubdata by creating a transaction with infinite gas limit.
            // And getting how much pubdata it used.

            // In theory, if the transaction has failed with such large gas limit, we could have returned an API error here right away,
            // but doing it later on keeps the code more lean.
            let (result, _) = self
                .estimate_gas_step(
                    vm_permit.clone(),
                    tx.clone(),
                    max_gas_limit,
                    gas_per_pubdata_byte as u32,
                    fee_input,
                    block_args,
                    base_fee,
                    protocol_version.into(),
                    state_override.clone(),
                )
                .await
                .context("estimate_gas step failed")?;

            // It is assumed that there is no overflow here
            let gas_for_pubdata =
                (result.statistics.pubdata_published as u64) * gas_per_pubdata_byte;
            (Some(result.statistics.gas_used), gas_for_pubdata)
        };

        // We are using binary search to find the minimal values of gas_limit under which
        // the transaction succeeds
        let mut lower_bound = gas_used.unwrap_or(0);
        let mut upper_bound = MAX_L2_TX_GAS_LIMIT;
        tracing::trace!(
            "preparation took {:?}, starting binary search",
            estimation_started_at.elapsed()
        );
        let mut number_of_iterations = 0_usize;

        if let Some(gas_used) = gas_used {
            // Perform an initial search iteration with the pivot slightly greater than `gas_used` to account for 63/64 rule for far calls etc.
            // If the transaction succeeds, it will discard most of the search space at once.
            let iteration_started_at = Instant::now();
            let optimistic_gas_limit =
                (gas_used * 6 / 5).min(upper_bound) + additional_gas_for_pubdata;

            let (result, _) = self
                .estimate_gas_step(
                    vm_permit.clone(),
                    tx.clone(),
                    optimistic_gas_limit,
                    gas_per_pubdata_byte as u32,
                    fee_input,
                    block_args,
                    base_fee,
                    protocol_version.into(),
                    state_override.clone(),
                )
                .await
                .context("estimate_gas step failed")?;
            Self::adjust_search_bounds(
                &mut lower_bound,
                &mut upper_bound,
                optimistic_gas_limit,
                &result,
            );

            tracing::trace!(
                "iteration {number_of_iterations} took {:?}. lower_bound: {lower_bound}, upper_bound: {upper_bound}",
                iteration_started_at.elapsed()
            );
            number_of_iterations += 1;
        }

        while lower_bound + acceptable_overestimation < upper_bound {
            let mid = (lower_bound + upper_bound) / 2;
            // There is no way to distinct between errors due to out of gas
            // or normal execution errors, so we just hope that increasing the
            // gas limit will make the transaction successful
            let iteration_started_at = Instant::now();
            let try_gas_limit = additional_gas_for_pubdata + mid;
            let (result, _) = self
                .estimate_gas_step(
                    vm_permit.clone(),
                    tx.clone(),
                    try_gas_limit,
                    gas_per_pubdata_byte as u32,
                    fee_input,
                    block_args,
                    base_fee,
                    protocol_version.into(),
                    state_override.clone(),
                )
                .await
                .context("estimate_gas step failed")?;
            Self::adjust_search_bounds(&mut lower_bound, &mut upper_bound, try_gas_limit, &result);

            tracing::trace!(
                "iteration {number_of_iterations} took {:?}. lower_bound: {lower_bound}, upper_bound: {upper_bound}",
                iteration_started_at.elapsed()
            );
            number_of_iterations += 1;
        }
        SANDBOX_METRICS
            .estimate_gas_binary_search_iterations
            .observe(number_of_iterations);

        let suggested_gas_limit =
            ((upper_bound + additional_gas_for_pubdata) as f64 * estimated_fee_scale_factor) as u64;
        let (result, tx_metrics) = self
            .estimate_gas_step(
                vm_permit,
                tx.clone(),
                suggested_gas_limit,
                gas_per_pubdata_byte as u32,
                fee_input,
                block_args,
                base_fee,
                protocol_version.into(),
                state_override,
            )
            .await
            .context("final estimate_gas step failed")?;

        result.into_api_call_result()?;
        self.ensure_tx_executable(&tx, &tx_metrics, false)?;

        // Now, we need to calculate the final overhead for the transaction.
        let overhead = derive_overhead(
            suggested_gas_limit,
            gas_per_pubdata_byte as u32,
            tx.encoding_len(),
            tx.tx_format() as u8,
            protocol_version.into(),
        );

        let full_gas_limit = match suggested_gas_limit.overflowing_add(overhead.into()) {
            (value, false) => {
                if value > max_gas_limit {
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

        let gas_for_pubdata = u64::from(tx_metrics.pubdata_published) * gas_per_pubdata_byte;
        let estimated_gas_for_pubdata =
            (gas_for_pubdata as f64 * estimated_fee_scale_factor) as u64;

        tracing::debug!(
            "gas for pubdata: {estimated_gas_for_pubdata}, computational gas: {}, overhead gas: {overhead} \
            (with params base_fee: {base_fee}, gas_per_pubdata_byte: {gas_per_pubdata_byte}) \
            estimated_fee_scale_factor: {estimated_fee_scale_factor}",
            suggested_gas_limit - estimated_gas_for_pubdata,
        );

        Ok(Fee {
            max_fee_per_gas: base_fee.into(),
            max_priority_fee_per_gas: 0u32.into(),
            gas_limit: full_gas_limit.into(),
            gas_per_pubdata_limit: gas_per_pubdata_byte.into(),
        })
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
                return Err(SubmitTxError::InsufficientFundsForTransfer);
            }
        }
        Ok(())
    }

    fn adjust_search_bounds(
        lower_bound: &mut u64,
        upper_bound: &mut u64,
        pivot: u64,
        result: &VmExecutionResultAndLogs,
    ) {
        // For now, we don't discern between "out of gas" and other failure reasons since it's difficult in the general case.
        if result.result.is_failed() {
            *lower_bound = pivot + 1;
        } else {
            *upper_bound = pivot;
        }
    }
}
