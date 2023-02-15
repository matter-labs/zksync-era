//! Helper module to submit transactions into the zkSync Network.

// Built-in uses
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

// External uses
use bigdecimal::BigDecimal;
use governor::clock::MonotonicClock;
use governor::middleware::NoOpMiddleware;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};

use vm::vm_with_bootloader::{derive_base_fee_and_gas_per_pubdata, TxExecutionMode};
use vm::zk_evm::zkevm_opcode_defs::system_params::MAX_PUBDATA_PER_BLOCK;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_dal::transactions_dal::L2TxSubmissionResult;
use zksync_eth_client::clients::http_client::EthereumClient;

use zksync_types::fee::TransactionExecutionMetrics;
use zksync_types::utils::storage_key_for_eth_balance;
use zksync_types::{
    FAIR_L2_GAS_PRICE, MAX_GAS_PER_PUBDATA_BYTE, MAX_L2_TX_GAS_LIMIT, MAX_NEW_FACTORY_DEPS,
};
// Workspace uses
use zksync_types::{
    api, fee::Fee, l2::error::TxCheckError::TxDuplication, AccountTreeId, Address, L2ChainId, U256,
};
use zksync_utils::h256_to_u256;
pub mod error;
// Local uses
use crate::api_server::execution_sandbox::{
    execute_tx_with_pending_state, get_pubdata_for_factory_deps, validate_tx_with_pending_state,
    SandboxExecutionError,
};

use crate::fee_ticker::{error::TickerError, FeeTicker, TokenPriceRequestType};
use crate::gas_adjuster::GasAdjuster;
use crate::gas_tracker::gas_count_from_tx_and_metrics;
use crate::state_keeper::seal_criteria::{SealManager, SealResolution};

pub use error::SubmitTxError;
use zksync_config::ZkSyncConfig;
use zksync_dal::ConnectionPool;
use zksync_types::{l2::L2Tx, tx::ExecutionMetrics, Nonce};

pub struct TxSenderInner {
    pub master_connection_pool: ConnectionPool,
    pub replica_connection_pool: ConnectionPool,
    pub fee_account_addr: Address,
    pub chain_id: L2ChainId,
    pub gas_price_scale_factor: f64,
    pub max_nonce_ahead: u32,
    pub max_single_tx_gas: u32,
    pub rate_limiter:
        Option<RateLimiter<NotKeyed, InMemoryState, MonotonicClock, NoOpMiddleware<Instant>>>,
    // Used to keep track of gas prices for the fee ticker.
    pub gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
    pub state_keeper_config: StateKeeperConfig,
}

#[derive(Clone)]
pub struct TxSender(pub Arc<TxSenderInner>);

impl std::fmt::Debug for TxSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxSender").finish()
    }
}

impl TxSender {
    pub fn new(
        config: &ZkSyncConfig,
        master_connection_pool: ConnectionPool,
        replica_connection_pool: ConnectionPool,
        gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
    ) -> Self {
        let rate_limiter = config
            .api
            .web3_json_rpc
            .transactions_per_sec_limit
            .map(|value| {
                RateLimiter::direct_with_clock(
                    Quota::per_second(NonZeroU32::new(value).unwrap()),
                    &MonotonicClock::default(),
                )
            });

        Self(Arc::new(TxSenderInner {
            chain_id: L2ChainId(config.chain.eth.zksync_network_id),
            master_connection_pool,
            replica_connection_pool,
            fee_account_addr: config.chain.state_keeper.fee_account_addr,
            max_nonce_ahead: config.api.web3_json_rpc.max_nonce_ahead,
            gas_price_scale_factor: config.api.web3_json_rpc.gas_price_scale_factor,
            max_single_tx_gas: config.chain.state_keeper.max_single_tx_gas,
            rate_limiter,
            gas_adjuster,
            state_keeper_config: config.chain.state_keeper.clone(),
        }))
    }

    #[tracing::instrument(skip(self, tx))]
    pub fn submit_tx(&self, tx: L2Tx) -> Result<L2TxSubmissionResult, SubmitTxError> {
        if let Some(rate_limiter) = &self.0.rate_limiter {
            if rate_limiter.check().is_err() {
                return Err(SubmitTxError::RateLimitExceeded);
            }
        }
        let mut stage_started_at = Instant::now();

        if tx.common_data.fee.gas_limit > U256::from(u32::MAX)
            || tx.common_data.fee.gas_per_pubdata_limit > U256::from(u32::MAX)
        {
            return Err(SubmitTxError::GasLimitIsTooBig);
        }

        let _maximal_allowed_overhead = 0;

        if tx.common_data.fee.gas_limit
            > U256::from(self.0.state_keeper_config.max_allowed_l2_tx_gas_limit)
        {
            vlog::info!(
                "Submitted Tx is Unexecutable {:?} because of GasLimitIsTooBig {}",
                tx.hash(),
                tx.common_data.fee.gas_limit,
            );
            return Err(SubmitTxError::GasLimitIsTooBig);
        }
        if tx.common_data.fee.max_fee_per_gas < FAIR_L2_GAS_PRICE.into() {
            vlog::info!(
                "Submitted Tx is Unexecutable {:?} because of MaxFeePerGasTooLow {}",
                tx.hash(),
                tx.common_data.fee.max_fee_per_gas
            );
            return Err(SubmitTxError::MaxFeePerGasTooLow);
        }
        if tx.common_data.fee.max_fee_per_gas < tx.common_data.fee.max_priority_fee_per_gas {
            vlog::info!(
                "Submitted Tx is Unexecutable {:?} because of MaxPriorityFeeGreaterThanMaxFee {}",
                tx.hash(),
                tx.common_data.fee.max_fee_per_gas
            );
            return Err(SubmitTxError::MaxPriorityFeeGreaterThanMaxFee);
        }
        if tx.execute.factory_deps_length() > MAX_NEW_FACTORY_DEPS {
            return Err(SubmitTxError::TooManyFactoryDependencies(
                tx.execute.factory_deps_length(),
                MAX_NEW_FACTORY_DEPS,
            ));
        }

        // We still double-check the nonce manually
        // to make sure that only the correct nonce is submitted and the transaction's hashes never repeat
        self.validate_account_nonce(&tx)?;

        // Even though without enough balance the tx will not pass anyway
        // we check the user for enough balance explicitly here for better DevEx.
        self.validate_enough_balance(&tx)?;

        metrics::histogram!("api.web3.submit_tx", stage_started_at.elapsed(), "stage" => "1_validate");
        stage_started_at = Instant::now();

        let l1_gas_price = self.0.gas_adjuster.estimate_effective_gas_price();
        let fair_l2_gas_price = FAIR_L2_GAS_PRICE;

        let (tx_metrics, _) = execute_tx_with_pending_state(
            &self.0.replica_connection_pool,
            tx.clone(),
            AccountTreeId::new(self.0.fee_account_addr),
            TxExecutionMode::EthCall,
            Some(tx.nonce()),
            U256::zero(),
            l1_gas_price,
            FAIR_L2_GAS_PRICE,
            Some(tx.common_data.fee.max_fee_per_gas.as_u64()),
        );

        vlog::info!(
            "Submit tx {:?} with execution metrics {:?}",
            tx.hash(),
            tx_metrics
        );
        metrics::histogram!("api.web3.submit_tx", stage_started_at.elapsed(), "stage" => "2_dry_run");
        stage_started_at = Instant::now();

        let validation_result = validate_tx_with_pending_state(
            &self.0.replica_connection_pool,
            tx.clone(),
            AccountTreeId::new(self.0.fee_account_addr),
            TxExecutionMode::VerifyExecute,
            Some(tx.nonce()),
            U256::zero(),
            l1_gas_price,
            fair_l2_gas_price,
            Some(tx.common_data.fee.max_fee_per_gas.as_u64()),
        );

        metrics::histogram!("api.web3.submit_tx", stage_started_at.elapsed(), "stage" => "3_verify_execute");
        stage_started_at = Instant::now();

        if let Err(err) = validation_result {
            return Err(err.into());
        }

        self.ensure_tx_executable(&tx, &tx_metrics, true)?;

        let nonce = tx.common_data.nonce.0;
        let hash = tx.hash();
        let expected_nonce = self.get_expected_nonce(&tx);
        let submission_res_handle = self
            .0
            .master_connection_pool
            .access_storage_blocking()
            .transactions_dal()
            .insert_transaction_l2(tx, tx_metrics);

        let status: String;
        let submission_result = match submission_res_handle {
            L2TxSubmissionResult::AlreadyExecuted => {
                status = "already_executed".to_string();
                Err(SubmitTxError::NonceIsTooLow(
                    expected_nonce.0,
                    expected_nonce.0 + self.0.max_nonce_ahead,
                    nonce,
                ))
            }
            L2TxSubmissionResult::Duplicate => {
                status = "duplicated".to_string();
                Err(SubmitTxError::IncorrectTx(TxDuplication(hash)))
            }
            _ => {
                metrics::histogram!("api.web3.submit_tx", stage_started_at.elapsed(), "stage" => "4_db_insert");
                status = format!(
                    "mempool_{}",
                    submission_res_handle.to_string().to_lowercase()
                );
                Ok(submission_res_handle)
            }
        };

        metrics::counter!(
            "server.processed_txs",
            1,
            "stage" => status
        );

        submission_result
    }

    fn validate_account_nonce(&self, tx: &L2Tx) -> Result<(), SubmitTxError> {
        let expected_nonce = self.get_expected_nonce(tx);

        if tx.common_data.nonce.0 < expected_nonce.0 {
            Err(SubmitTxError::NonceIsTooLow(
                expected_nonce.0,
                expected_nonce.0 + self.0.max_nonce_ahead,
                tx.nonce().0,
            ))
        } else if !(expected_nonce.0..=(expected_nonce.0 + self.0.max_nonce_ahead))
            .contains(&tx.common_data.nonce.0)
        {
            Err(SubmitTxError::NonceIsTooHigh(
                expected_nonce.0,
                expected_nonce.0 + self.0.max_nonce_ahead,
                tx.nonce().0,
            ))
        } else {
            Ok(())
        }
    }

    fn get_expected_nonce(&self, tx: &L2Tx) -> Nonce {
        self.0
            .replica_connection_pool
            .access_storage_blocking()
            .storage_web3_dal()
            .get_address_historical_nonce(
                tx.initiator_account(),
                api::BlockId::Number(api::BlockNumber::Latest),
            )
            .unwrap()
            .map(|n| Nonce(n.as_u32()))
            .unwrap()
    }

    fn validate_enough_balance(&self, tx: &L2Tx) -> Result<(), SubmitTxError> {
        let paymaster = tx.common_data.paymaster_params.paymaster;

        // The paymaster is expected to pay for the tx,
        // whatever balance the user has, we don't care.
        if paymaster != Address::default() {
            return Ok(());
        }

        let eth_balance_key = storage_key_for_eth_balance(&tx.common_data.initiator_address);

        let balance = self
            .0
            .replica_connection_pool
            .access_storage_blocking()
            .storage_dal()
            .get_by_key(&eth_balance_key)
            .unwrap_or_default();
        let balance = h256_to_u256(balance);

        // Estimate the minimum fee price user will agree to.
        let gas_price = std::cmp::min(
            tx.common_data.fee.max_fee_per_gas,
            U256::from(FAIR_L2_GAS_PRICE) + tx.common_data.fee.max_priority_fee_per_gas,
        );
        let max_fee = tx.common_data.fee.gas_limit * gas_price;
        let max_fee_and_value = max_fee + tx.execute.value;

        if balance < max_fee_and_value {
            Err(SubmitTxError::NotEnoughBalanceForFeeValue(
                balance,
                max_fee,
                tx.execute.value,
            ))
        } else {
            Ok(())
        }
    }

    /// Given the gas per pubdata limit signed by the user, returns
    /// the gas per pubdata byte that should be used in the block for simulation
    pub fn validate_gas_per_pubdata_byte(
        &self,
        agreed_by_user: U256,
    ) -> Result<u32, SubmitTxError> {
        // The user has agreed an a higher gas price than it is even possible to have in block.
        // While we could just let it go, it is better to ensure that users know what they are doing.
        if agreed_by_user > U256::from(u32::MAX) {
            return Err(SubmitTxError::FeePerPubdataByteTooHigh);
        }

        // It is now safe to convert here
        let agreed_by_user = agreed_by_user.as_u32();

        // This check is needed to filter out unrealistic transactions that will reside in mempool forever.
        // If transaction has such limit set, most likely it was done manually or there is some mistake
        // in user's code. This check is only needed for better UX.
        const MIN_GAS_PER_PUBDATA_LIMIT: u32 = 10; // At 0.1 gwei per l2 gas it gives us max 1 gwei of l1 gas price.
        if agreed_by_user < MIN_GAS_PER_PUBDATA_LIMIT {
            return Err(SubmitTxError::UnrealisticPubdataPriceLimit);
        }

        let l1_gas_price = self.0.gas_adjuster.estimate_effective_gas_price();
        let suggested_gas_price_per_pubdata =
            derive_base_fee_and_gas_per_pubdata(l1_gas_price, FAIR_L2_GAS_PRICE).1 as u32;

        // If user provided gas per pubdata limit lower than currently suggested
        // by the server, the users' transaction will not be included in the blocks right away
        // but it will stay in mempool. We still have to simulate it somehow, so we'll use the user's
        // provided pubdata price
        let result = agreed_by_user.min(suggested_gas_price_per_pubdata);

        Ok(result)
    }

    pub fn get_txs_fee_in_wei(
        &self,
        mut tx: L2Tx,
        estimated_fee_scale_factor: f64,
        acceptable_overestimation: u32,
    ) -> Result<Fee, SubmitTxError> {
        let l1_gas_price = {
            let effective_gas_price = self.0.gas_adjuster.estimate_effective_gas_price();
            ((effective_gas_price as f64) * self.0.gas_price_scale_factor) as u64
        };

        let (base_fee, gas_per_pubdata_byte) =
            derive_base_fee_and_gas_per_pubdata(l1_gas_price, FAIR_L2_GAS_PRICE);

        // If no signature has been provided, we will use the correctly-formatted EOA
        // dummy signature
        if tx.common_data.signature.is_empty() {
            tx.common_data.signature = vec![0u8; 65];
            tx.common_data.signature[64] = 27;
        }

        /// The calculated transaction length below does not include the signature and most likely
        /// most of the paymasterInput. We will assume that those take no more than 64 slots (2048 bytes)
        /// in total. If they do, the user should provide manually a higher gasLimit.
        ///
        /// In the future, a more advanced protocol for getting the fee for transactions from custom accounts
        /// will be used.
        const TX_LENGTH_OVERHEAD: usize = 64;

        let _tx_encoded_len = tx.abi_encoding_len() + TX_LENGTH_OVERHEAD;

        // We already know how many gas is needed to cover for the publishing of the bytecodes.
        let gas_for_bytecodes_pubdata = {
            let pubdata_for_factory_deps = get_pubdata_for_factory_deps(
                &self.0.replica_connection_pool,
                &tx.execute.factory_deps,
            );
            if pubdata_for_factory_deps > MAX_PUBDATA_PER_BLOCK {
                return Err(SubmitTxError::Unexecutable(
                    "exceeds limit for published pubdata".to_string(),
                ));
            }
            pubdata_for_factory_deps * (gas_per_pubdata_byte as u32)
        };

        // We are using binary search to find the minimal values of gas_limit under which
        // the transaction succeedes
        let mut lower_bound = 0;
        let mut upper_bound = MAX_L2_TX_GAS_LIMIT as u32;

        tx.common_data.fee.gas_per_pubdata_limit = MAX_GAS_PER_PUBDATA_BYTE.into();

        // Given the gas_limit to be used for the body of the transaction,
        // returns the result for executing the transaction with such gas_limit
        let mut execute = |tx_gas_limit: u32| {
            let gas_limit_with_overhead = tx_gas_limit;

            tx.common_data.fee.gas_limit = gas_limit_with_overhead.into();
            let initial_paid_fee =
                U256::from(gas_limit_with_overhead) * tx.common_data.fee.max_fee_per_gas;

            let (tx_metrics, exec_result) = execute_tx_with_pending_state(
                &self.0.replica_connection_pool,
                tx.clone(),
                AccountTreeId::new(self.0.fee_account_addr),
                TxExecutionMode::EstimateFee,
                Some(tx.nonce()),
                initial_paid_fee,
                l1_gas_price,
                FAIR_L2_GAS_PRICE,
                Some(tx.common_data.fee.max_fee_per_gas.as_u64()),
            );

            self.ensure_tx_executable(&tx, &tx_metrics, false)
                .map_err(|err| {
                    let err_message = match err {
                        SubmitTxError::Unexecutable(err_message) => err_message,
                        _ => unreachable!(),
                    };

                    SandboxExecutionError::Unexecutable(err_message)
                })?;

            exec_result
        };

        let mut number_of_iterations = 0usize;
        while lower_bound + acceptable_overestimation < upper_bound {
            let mid = (lower_bound + upper_bound) / 2;

            // There is no way to distinct between errors due to out of gas
            // or normal exeuction errors, so we just hope that increasing the
            // gas limit will make the transaction successful
            if execute(gas_for_bytecodes_pubdata + mid).is_err() {
                lower_bound = mid + 1;
            } else {
                upper_bound = mid;
            }

            number_of_iterations += 1;
        }
        metrics::histogram!(
            "api.web3.estimate_gas_binary_search_iterations",
            number_of_iterations as f64
        );

        let tx_body_gas_limit = std::cmp::min(
            MAX_L2_TX_GAS_LIMIT as u32,
            ((upper_bound as f64) * estimated_fee_scale_factor) as u32,
        );

        match execute(tx_body_gas_limit + gas_for_bytecodes_pubdata) {
            Err(err) => Err(err.into()),
            Ok(_) => {
                let overhead = 0;

                let full_gas_limit =
                    match tx_body_gas_limit.overflowing_add(gas_for_bytecodes_pubdata + overhead) {
                        (_, true) => {
                            return Err(SubmitTxError::CannotEstimateTransaction(
                                "exceeds block gas limit".to_string(),
                            ))
                        }
                        (x, _) => x,
                    };

                Ok(Fee {
                    max_fee_per_gas: base_fee.into(),
                    max_priority_fee_per_gas: 0u32.into(),
                    gas_limit: full_gas_limit.into(),
                    gas_per_pubdata_limit: gas_per_pubdata_byte.into(),
                })
            }
        }
    }

    pub fn token_price(
        &self,
        request_type: TokenPriceRequestType,
        l2_token_address: Address,
    ) -> Result<BigDecimal, TickerError> {
        let mut storage = self.0.replica_connection_pool.access_storage_blocking();
        let mut tokens_web3_dal = storage.tokens_web3_dal();
        FeeTicker::get_l2_token_price(&mut tokens_web3_dal, request_type, &l2_token_address)
    }

    pub fn gas_price(&self) -> u64 {
        let gas_price = self.0.gas_adjuster.estimate_effective_gas_price();

        derive_base_fee_and_gas_per_pubdata(
            (gas_price as f64 * self.0.gas_price_scale_factor).round() as u64,
            FAIR_L2_GAS_PRICE,
        )
        .0
    }

    fn ensure_tx_executable(
        &self,
        transaction: &L2Tx,
        tx_metrics: &TransactionExecutionMetrics,
        log_message: bool,
    ) -> Result<(), SubmitTxError> {
        let execution_metrics = ExecutionMetrics {
            initial_storage_writes: tx_metrics.initial_storage_writes,
            repeated_storage_writes: tx_metrics.repeated_storage_writes,
            published_bytecode_bytes: tx_metrics.published_bytecode_bytes,
            l2_l1_long_messages: tx_metrics.l2_l1_long_messages,
            l2_l1_logs: tx_metrics.l2_l1_logs,
            contracts_deployed: tx_metrics.contracts_deployed,
            contracts_used: tx_metrics.contracts_used,
            gas_used: tx_metrics.gas_used,
            storage_logs: tx_metrics.storage_logs,
            vm_events: tx_metrics.vm_events,
            total_log_queries: tx_metrics.total_log_queries,
            cycles_used: tx_metrics.cycles_used,
        };

        // In api server it's ok to expect that all writes are initial it's safer
        let tx_gas_count =
            gas_count_from_tx_and_metrics(&transaction.clone().into(), &execution_metrics);

        for sealer in &SealManager::get_default_sealers() {
            let seal_resolution = sealer.should_seal(
                &self.0.state_keeper_config,
                0u128,
                1,
                execution_metrics,
                execution_metrics,
                tx_gas_count,
                tx_gas_count,
            );
            if matches!(seal_resolution, SealResolution::Unexecutable(_)) {
                let message = format!(
                    "Tx is Unexecutable because of {} with execution values {:?} and gas {:?}",
                    sealer.prom_criterion_name(),
                    execution_metrics,
                    tx_gas_count
                );

                if log_message {
                    vlog::info!("{:#?} {}", transaction.hash(), message);
                }

                return Err(SubmitTxError::Unexecutable(message));
            }
        }
        Ok(())
    }
}
