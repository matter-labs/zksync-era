use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::{channel::mpsc, SinkExt};
use tokio::sync::RwLock;
use zksync::{error::ClientError, operations::SyncTransactionHandle, HttpClient};
use zksync_contracts::test_contracts::LoadnextContractExecutionParams;
use zksync_types::{api::TransactionReceipt, Address, Nonce, H256, U256, U64};
use zksync_web3_decl::jsonrpsee::core::ClientError as CoreError;

use crate::{
    account::tx_command_executor::SubmitResult,
    account_pool::{AddressPool, TestWallet},
    command::{ExpectedOutcome, IncorrectnessModifier, TxCommand, TxType},
    config::{LoadtestConfig, RequestLimiters},
    constants::{MAX_L1_TRANSACTIONS, POLLING_INTERVAL},
    report::{Report, ReportBuilder, ReportLabel},
    utils::format_gwei,
};

mod api_request_executor;
mod pubsub_executor;
mod tx_command_executor;

/// Error returned when the load test is aborted.
#[derive(Debug)]
struct Aborted;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionType {
    L1,
    L2,
}

/// Tx that has been sent to the server but has not yet received a receipt
#[derive(Debug, Clone)]
struct InflightTx {
    tx_hash: H256,
    attempt: usize,
    start: Instant,
    command: TxCommand,
}

/// Account lifespan represents a flow of a single account:
/// it will send transactions, both correct and incorrect, and will check
/// whether outcome matches expected one.
///
/// This structure is expected to not care about the server behavior; even if the server is down, it will only cause
/// performed actions to be considered failed.
#[derive(Debug, Clone)]
pub struct AccountLifespan {
    /// Wallet used to perform the test.
    pub wallet: TestWallet,
    config: LoadtestConfig,
    contract_execution_params: LoadnextContractExecutionParams,
    /// Pool of account addresses, used to generate commands.
    addresses: AddressPool,
    /// Successful transactions, required for requesting API
    successfully_sent_txs: Arc<RwLock<Vec<H256>>>,
    /// L1 ERC-20 token used in the test.
    main_l1_token: Address,
    /// L2 ERC-20 token used in the test.
    main_l2_token: Address,
    /// Address of the paymaster used in the test.
    paymaster_address: Address,
    /// Channel for sending reports about performed operations.
    report_sink: mpsc::Sender<Report>,
    /// Pool of sent but not yet executed txs
    inflight_txs: VecDeque<InflightTx>,
    /// Current account nonce, it is None at the beginning and will be set after the first transaction
    current_nonce: Option<Nonce>,
}

impl AccountLifespan {
    pub fn new(
        config: &LoadtestConfig,
        contract_execution_params: LoadnextContractExecutionParams,
        addresses: AddressPool,
        test_account: TestWallet,
        report_sink: mpsc::Sender<Report>,
        main_l2_token: Address,
        paymaster_address: Address,
    ) -> Self {
        Self {
            wallet: test_account,
            config: config.clone(),
            contract_execution_params,
            addresses,
            successfully_sent_txs: Default::default(),
            main_l1_token: config.main_token,
            main_l2_token,
            paymaster_address,
            report_sink,
            inflight_txs: Default::default(),
            current_nonce: None,
        }
    }

    pub async fn run(self, limiters: &RequestLimiters) {
        let duration = self.config.duration();
        let tx_execution_task = self.clone().run_tx_execution();
        let api_requests_task = self.clone().run_api_requests_task(limiters);

        tokio::select! {
            result = tx_execution_task => {
                tracing::trace!("Transaction execution task finished with {result:?}");
            },
            result = api_requests_task => {
                tracing::trace!("API requests task finished with {result:?}");
            },
            result = self.run_pubsub_task(limiters) => {
                tracing::trace!("PubSub task finished with {result:?}");
            },
            () = tokio::time::sleep(duration) => {}
        }
    }

    async fn run_tx_execution(mut self) -> Result<(), Aborted> {
        // Every account starts with deploying a contract.
        let deploy_command = TxCommand {
            command_type: TxType::DeployContract,
            modifier: IncorrectnessModifier::None,
            to: Address::zero(),
            amount: U256::zero(),
        };
        if self.config.is_evm() {
            self.execute_command_evm(deploy_command.clone()).await?;
        } else {
            self.execute_command(deploy_command.clone()).await?;
        }
        self.wait_for_all_inflight_tx().await?;

        let mut timer = tokio::time::interval(POLLING_INTERVAL);
        let mut l1_tx_count = 0;
        loop {
            let command = self.generate_command();
            let is_l1_transaction =
                matches!(command.command_type, TxType::L1Execute | TxType::Deposit);
            if is_l1_transaction && l1_tx_count >= MAX_L1_TRANSACTIONS {
                continue; // Skip command to not run out of Ethereum on L1
            }

            // The new transaction should be sent only if mempool is not full
            loop {
                if self.inflight_txs.len() >= self.config.max_inflight_txs {
                    timer.tick().await;
                    self.check_inflight_txs().await?;
                } else {
                    self.execute_command(command).await?;
                    l1_tx_count += u64::from(is_l1_transaction);
                    break;
                }
            }
        }
    }

    async fn wait_for_all_inflight_tx(&mut self) -> Result<(), Aborted> {
        let mut timer = tokio::time::interval(POLLING_INTERVAL);
        while !self.inflight_txs.is_empty() {
            timer.tick().await;
            self.check_inflight_txs().await?;
        }
        Ok(())
    }

    async fn check_inflight_txs(&mut self) -> Result<(), Aborted> {
        // No need to wait for confirmation for all tx, one check for each tx is enough.
        // If some txs haven't been processed yet, we'll check them in the next iteration.
        // Due to natural sleep for sending tx, usually more than 1 tx can be already
        // processed and have a receipt
        let start = Instant::now();
        tracing::trace!(
            "Account {:?}: check_inflight_txs len {:?}",
            self.wallet.wallet.address(),
            self.inflight_txs.len()
        );

        while let Some(tx) = self.inflight_txs.pop_front() {
            let receipt = self.get_tx_receipt_for_committed_block(tx.tx_hash).await;
            match receipt {
                Ok(Some(transaction_receipt)) => {
                    let label = self.verify_receipt(
                        &transaction_receipt,
                        &tx.command.modifier.expected_outcome(),
                    );
                    let gas_used = transaction_receipt.gas_used.unwrap_or(U256::zero());
                    let effective_gas_price = transaction_receipt
                        .effective_gas_price
                        .unwrap_or(U256::zero());
                    tracing::debug!(
                        "Account {:?}: tx included. Total fee: {}, gas used: {gas_used}gas, \
                         gas price: {effective_gas_price} WEI. Latency {:?} at attempt {:?}",
                        self.wallet.wallet.address(),
                        format_gwei(gas_used * effective_gas_price),
                        tx.start.elapsed(),
                        tx.attempt,
                    );
                    self.report(label, tx.start.elapsed(), tx.attempt, tx.command)
                        .await?;
                }
                other => {
                    tracing::trace!(
                        "Account {:?}: check_inflight_txs tx not yet included: {other:?}",
                        self.wallet.wallet.address()
                    );
                    self.inflight_txs.push_front(tx);
                    break;
                }
            }
        }
        tracing::trace!(
            "Account {:?}: check_inflight_txs complete {:?}",
            self.wallet.wallet.address(),
            start.elapsed()
        );
        Ok(())
    }

    fn verify_receipt(
        &self,
        transaction_receipt: &TransactionReceipt,
        expected_outcome: &ExpectedOutcome,
    ) -> ReportLabel {
        match expected_outcome {
            ExpectedOutcome::TxSucceed if transaction_receipt.status == U64::one() => {
                // If it was a successful `DeployContract` transaction, set the contract
                // address for subsequent usage by `Execute`.
                if let Some(address) = transaction_receipt.contract_address {
                    // An error means that the contract is already initialized.
                    self.wallet.deployed_contract_address.set(address).ok();
                }

                // Transaction succeed and it should have.
                ReportLabel::done()
            }
            ExpectedOutcome::TxRejected if transaction_receipt.status == U64::zero() => {
                // Transaction failed and it should have.
                ReportLabel::done()
            }
            other => {
                // Transaction status didn't match expected one.
                let error = format!(
                    "Unexpected transaction status: expected {other:#?} because of \
                     modifier {expected_outcome:?}, receipt {transaction_receipt:#?}"
                );
                ReportLabel::failed(error)
            }
        }
    }

    /// Executes a command with support of retries:
    /// If command fails due to the network/API error, it will be retried multiple times
    /// before considering it completely failed. Such an approach makes us a bit more resilient to
    /// volatile errors such as random connection drop or insufficient fee error.
    async fn execute_command(&mut self, command: TxCommand) -> Result<(), Aborted> {
        // We consider API errors to be somewhat likely, thus we will retry the operation if it fails
        // due to connection issues.
        const MAX_RETRIES: usize = 3;

        let mut attempt = 0;
        loop {
            let start = Instant::now();
            let result = self.execute_tx_command(&command).await;

            let submit_result = match result {
                Ok(result) => result,
                Err(err) if Self::should_retry(&err) => {
                    if attempt < MAX_RETRIES {
                        tracing::warn!("Error while sending tx: {err}. Retrying...");
                        // Retry operation.
                        attempt += 1;
                        continue;
                    }

                    // We reached the maximum amount of retries.
                    let error = format!("Retries limit reached. Latest error: {err}");
                    SubmitResult::ReportLabel(ReportLabel::failed(error))
                }
                Err(err) => {
                    // Other kinds of errors should not be handled, we will just report them.
                    SubmitResult::ReportLabel(ReportLabel::failed(err.to_string()))
                }
            };

            match submit_result {
                SubmitResult::TxHash(tx_hash) => {
                    self.inflight_txs.push_back(InflightTx {
                        tx_hash,
                        start,
                        attempt,
                        command: command.clone(),
                    });
                    self.successfully_sent_txs.write().await.push(tx_hash)
                }
                SubmitResult::ReportLabel(label) => {
                    // Make a report if there was some problems sending tx
                    self.report(label, start.elapsed(), attempt, command)
                        .await?;
                }
            }

            // We won't continue the loop unless `continue` was manually called.
            break;
        }
        Ok(())
    }

    /// Executes a command with support of retries:
    /// If command fails due to the network/API error, it will be retried multiple times
    /// before considering it completely failed. Such an approach makes us a bit more resilient to
    /// volatile errors such as random connection drop or insufficient fee error.
    async fn execute_command_evm(&mut self, command: TxCommand) -> Result<(), Aborted> {
        // We consider API errors to be somewhat likely, thus we will retry the operation if it fails
        // due to connection issues.
        const MAX_RETRIES: usize = 3;

        let mut attempt = 0;
        loop {
            let start = Instant::now();
            let result = self.execute_deploy_evm().await;

            let submit_result = match result {
                Ok(result) => result,
                Err(err) if Self::should_retry(&err) => {
                    if attempt < MAX_RETRIES {
                        tracing::warn!("Error while sending tx: {err}. Retrying...");
                        // Retry operation.
                        attempt += 1;
                        continue;
                    }

                    // We reached the maximum amount of retries.
                    let error = format!("Retries limit reached. Latest error: {err}");
                    SubmitResult::ReportLabel(ReportLabel::failed(error))
                }
                Err(err) => {
                    // Other kinds of errors should not be handled, we will just report them.
                    SubmitResult::ReportLabel(ReportLabel::failed(err.to_string()))
                }
            };

            match submit_result {
                SubmitResult::TxHash(_) => {}
                SubmitResult::ReportLabel(label) => {
                    // Make a report if there was some problems sending tx
                    self.report(label, start.elapsed(), attempt, command)
                        .await?;
                }
            }

            // We won't continue the loop unless `continue` was manually called.
            break;
        }
        Ok(())
    }

    fn should_retry(err: &ClientError) -> bool {
        matches!(
            err,
            ClientError::NetworkError(_)
                | ClientError::OperationTimeout
                | ClientError::RpcError(CoreError::Transport(_) | CoreError::RequestTimeout)
        )
    }

    pub async fn reset_nonce(&mut self) {
        let nonce = Nonce(self.wallet.wallet.get_nonce().await.unwrap());
        self.current_nonce = Some(nonce);
    }

    /// Builds a report and sends it.
    async fn report(
        &mut self,
        label: ReportLabel,
        time: Duration,
        retries: usize,
        command: TxCommand,
    ) -> Result<(), Aborted> {
        if let ReportLabel::ActionFailed { error } = &label {
            tracing::error!(
                "Command failed: from {:?}, {command:#?} ({error})",
                self.wallet.wallet.address()
            )
        }

        let report = ReportBuilder::default()
            .label(label)
            .reporter(self.wallet.wallet.address())
            .time(time)
            .retries(retries)
            .action(command)
            .finish();
        self.send_report(report).await
    }

    async fn send_report(&mut self, report: Report) -> Result<(), Aborted> {
        if self.report_sink.send(report).await.is_err() {
            Err(Aborted)
        } else {
            Ok(())
        }
    }

    /// Generic submitter for zkSync network: it can operate individual transactions,
    /// as long as we can provide a `SyncTransactionHandle` to wait for the commitment and the
    /// execution result.
    /// Once result is obtained, it's compared to the expected operation outcome in order to check whether
    /// command was completed as planned.
    async fn submit(
        &mut self,
        modifier: IncorrectnessModifier,
        send_result: Result<SyncTransactionHandle<'_, HttpClient>, ClientError>,
    ) -> Result<SubmitResult, ClientError> {
        let expected_outcome = modifier.expected_outcome();

        let submit_result = match (expected_outcome, send_result) {
            (ExpectedOutcome::ApiRequestFailed, Ok(_)) => {
                // Transaction got accepted, but should have not been.
                let error = "Tx was accepted, but should have not been";
                SubmitResult::ReportLabel(ReportLabel::failed(error))
            }
            (_, Ok(handle)) => {
                // Transaction should have been accepted by API and it was; now wait for the commitment.
                SubmitResult::TxHash(handle.hash())
            }
            (ExpectedOutcome::ApiRequestFailed, Err(_)) => {
                // Transaction was expected to be rejected and it was.
                SubmitResult::ReportLabel(ReportLabel::done())
            }
            (_, Err(err)) => {
                // Transaction was expected to be accepted, but was rejected.
                if let ClientError::RpcError(CoreError::Call(err)) = &err {
                    let message = err.message();
                    if message.contains("nonce") {
                        self.reset_nonce().await;
                        return Ok(SubmitResult::ReportLabel(ReportLabel::skipped(message)));
                    }
                }

                let error =
                    format!("Tx should have been accepted, but got rejected. Reason: {err:?}");
                SubmitResult::ReportLabel(ReportLabel::failed(error))
            }
        };
        Ok(submit_result)
    }

    /// Prepares a list of random operations to be executed by an account.
    fn generate_command(&mut self) -> TxCommand {
        TxCommand::random(
            &mut self.wallet.rng,
            self.wallet.wallet.address(),
            &self.addresses,
        )
    }
}
