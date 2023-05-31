use futures::{channel::mpsc, FutureExt, SinkExt};
use std::{
    collections::VecDeque,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::RwLock, time::sleep};
use zksync_utils::test_utils::LoadnextContractExecutionParams;

use zksync::{error::ClientError, operations::SyncTransactionHandle, HttpClient};
use zksync_types::{
    api::{TransactionReceipt, U64},
    Address, Nonce, H256, U256,
};
use zksync_web3_decl::jsonrpsee;

use crate::{
    account::{explorer_api_executor::ExplorerApiClient, tx_command_executor::SubmitResult},
    account_pool::{AddressPool, TestWallet},
    command::{ExpectedOutcome, IncorrectnessModifier, TxCommand, TxType},
    config::LoadtestConfig,
    constants::POLLING_INTERVAL,
    report::{Report, ReportBuilder, ReportLabel},
};

mod api_request_executor;
mod explorer_api_executor;
mod pubsub_executor;
mod tx_command_executor;

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
    /// Client for explorer api
    pub explorer_client: ExplorerApiClient,
    config: LoadtestConfig,
    contract_execution_params: LoadnextContractExecutionParams,
    /// Pool of account addresses, used to generate commands.
    addresses: AddressPool,
    /// Successful transactions, required for requesting api
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
        let explorer_client = ExplorerApiClient {
            client: Default::default(),
            base_url: config.l2_explorer_api_address.clone(),
            last_sealed_block_number: None,
        };

        Self {
            wallet: test_account,
            explorer_client,
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

    pub async fn run(self) {
        let duration = self.config.duration();
        let mut tx_execution_task = Box::pin(self.clone().run_tx_execution()).fuse();
        let mut api_requests_task = Box::pin(self.clone().run_api_requests_task()).fuse();
        let mut api_explorer_requests_task =
            Box::pin(self.clone().run_explorer_api_requests_task()).fuse();
        let mut pubsub_task = Box::pin(self.run_pubsub_task()).fuse();
        let mut sleep_task = Box::pin(sleep(duration)).fuse();

        futures::select! {
            () = tx_execution_task => {},
            () = api_requests_task => {
                vlog::error!("API requests task unexpectedly finished first");
            },
            () = api_explorer_requests_task => {
                vlog::error!("Explorer API requests task unexpectedly finished first");
            },
            () = pubsub_task => {
                vlog::error!("PubSub task unexpectedly finished first");
            },
            () = sleep_task => {}
        }
    }

    async fn run_tx_execution(mut self) {
        // Every account starts with deploying a contract.
        let deploy_command = TxCommand {
            command_type: TxType::DeployContract,
            modifier: IncorrectnessModifier::None,
            to: Address::zero(),
            amount: U256::zero(),
        };
        self.execute_command(deploy_command.clone()).await;
        self.wait_for_all_inflight_tx().await;
        let mut timer = tokio::time::interval(POLLING_INTERVAL);
        loop {
            let command = self.generate_command();
            // The new transaction should be sent only if mempool is not full
            loop {
                if self.inflight_txs.len() >= self.config.max_inflight_txs {
                    timer.tick().await;
                    self.check_inflight_txs().await;
                } else {
                    self.execute_command(command.clone()).await;
                    break;
                }
            }
        }
    }

    async fn wait_for_all_inflight_tx(&mut self) {
        let mut timer = tokio::time::interval(POLLING_INTERVAL);
        while !self.inflight_txs.is_empty() {
            timer.tick().await;
            self.check_inflight_txs().await;
        }
    }

    async fn check_inflight_txs(&mut self) {
        // No need to wait for confirmation for all tx, one check for each tx is enough.
        // If some txs haven't been processed yet, we'll check them in the next iteration.
        // Due to natural sleep for sending tx, usually more than 1 tx can be already
        // processed and have a receipt
        let start = Instant::now();
        vlog::debug!(
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
                    vlog::trace!(
                        "Account {:?}: check_inflight_txs tx is included after {:?} attempt {:?}",
                        self.wallet.wallet.address(),
                        tx.start.elapsed(),
                        tx.attempt
                    );
                    self.report(label, tx.start.elapsed(), tx.attempt, tx.command)
                        .await;
                }
                other => {
                    vlog::debug!(
                        "Account {:?}: check_inflight_txs tx not yet included: {:?}",
                        self.wallet.wallet.address(),
                        other
                    );
                    self.inflight_txs.push_front(tx);
                    break;
                }
            }
        }
        vlog::debug!(
            "Account {:?}: check_inflight_txs complete {:?}",
            self.wallet.wallet.address(),
            start.elapsed()
        );
    }

    fn verify_receipt(
        &self,
        transaction_receipt: &TransactionReceipt,
        expected_outcome: &ExpectedOutcome,
    ) -> ReportLabel {
        match expected_outcome {
            ExpectedOutcome::TxSucceed if transaction_receipt.status == Some(U64::one()) => {
                // If it was a successful `DeployContract` transaction, set the contract
                // address for subsequent usage by `Execute`.
                if let Some(address) = transaction_receipt.contract_address {
                    // An error means that the contract is already initialized.
                    let _ = self.wallet.deployed_contract_address.set(address);
                }

                // Transaction succeed and it should have.
                ReportLabel::done()
            }
            ExpectedOutcome::TxRejected if transaction_receipt.status == Some(U64::zero()) => {
                // Transaction failed and it should have.
                ReportLabel::done()
            }
            other => {
                // Transaction status didn't match expected one.
                let error = format!(
                    "Unexpected transaction status: expected {:#?} because of modifier {:?}, receipt {:#?}",
                    other, expected_outcome, transaction_receipt
                );
                ReportLabel::failed(&error)
            }
        }
    }

    /// Executes a command with support of retries:
    /// If command fails due to the network/API error, it will be retried multiple times
    /// before considering it completely failed. Such an approach makes us a bit more resilient to
    /// volatile errors such as random connection drop or insufficient fee error.
    async fn execute_command(&mut self, command: TxCommand) {
        // We consider API errors to be somewhat likely, thus we will retry the operation if it fails
        // due to connection issues.
        const MAX_RETRIES: usize = 3;

        let mut attempt = 0;
        loop {
            let start = Instant::now();
            let result = self.execute_tx_command(&command).await;

            let submit_result = match result {
                Ok(result) => result,
                Err(ClientError::NetworkError(_)) | Err(ClientError::OperationTimeout) => {
                    if attempt < MAX_RETRIES {
                        vlog::warn!(
                            "Error while sending tx: {}. Retrying...",
                            result.unwrap_err()
                        );
                        // Retry operation.
                        attempt += 1;
                        continue;
                    }

                    // We reached the maximum amount of retries.
                    let error = format!(
                        "Retries limit reached. Latest error: {}",
                        result.unwrap_err()
                    );
                    SubmitResult::ReportLabel(ReportLabel::failed(&error))
                }
                Err(err) => {
                    // Other kinds of errors should not be handled, we will just report them.
                    SubmitResult::ReportLabel(ReportLabel::failed(&err.to_string()))
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
                    self.report(label, start.elapsed(), attempt, command).await
                }
            };

            // We won't continue the loop unless `continue` was manually called.
            break;
        }
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
    ) {
        if let ReportLabel::ActionFailed { error } = &label {
            vlog::error!(
                "Command failed: from {:?}, {:#?} (${})",
                self.wallet.wallet.address(),
                command,
                error
            )
        }

        let report = ReportBuilder::new()
            .label(label)
            .reporter(self.wallet.wallet.address())
            .time(time)
            .retries(retries)
            .action(command)
            .finish();

        if let Err(_err) = self.report_sink.send(report).await {
            // It's not that important if report will be skipped.
            vlog::trace!("Failed to send report to the sink");
        };
    }

    /// Generic submitter for zkSync network: it can operate individual transactions,
    /// as long as we can provide a `SyncTransactionHandle` to wait for the commitment and the
    /// execution result.
    /// Once result is obtained, it's compared to the expected operation outcome in order to check whether
    /// command was completed as planned.
    async fn submit<'a, F, Fut>(
        &'a mut self,
        modifier: IncorrectnessModifier,
        send: F,
    ) -> Result<SubmitResult, ClientError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<SyncTransactionHandle<'a, HttpClient>, ClientError>>,
    {
        let expected_outcome = modifier.expected_outcome();

        let send_result = send().await;

        let submit_result = match (expected_outcome, send_result) {
            (ExpectedOutcome::ApiRequestFailed, Ok(_handle)) => {
                // Transaction got accepted, but should have not been.
                let error = "Tx was accepted, but should have not been";
                SubmitResult::ReportLabel(ReportLabel::failed(error))
            }
            (_, Ok(handle)) => {
                // Transaction should have been accepted by API and it was; now wait for the commitment.
                SubmitResult::TxHash(handle.hash())
            }
            (ExpectedOutcome::ApiRequestFailed, Err(_error)) => {
                // Transaction was expected to be rejected and it was.
                SubmitResult::ReportLabel(ReportLabel::done())
            }
            (_, Err(err)) => {
                // Transaction was expected to be accepted, but was rejected.
                if let ClientError::RpcError(jsonrpsee::core::Error::Call(err)) = &err {
                    let message = match err {
                        jsonrpsee::types::error::CallError::InvalidParams(err) => err.to_string(),
                        jsonrpsee::types::error::CallError::Failed(err) => err.to_string(),
                        jsonrpsee::types::error::CallError::Custom(err) => {
                            err.message().to_string()
                        }
                    };
                    if message.contains("nonce") {
                        self.reset_nonce().await;
                        return Ok(SubmitResult::ReportLabel(ReportLabel::skipped(&message)));
                    }
                }

                let error = format!(
                    "Tx should have been accepted, but got rejected. Reason: {:?}",
                    err
                );
                SubmitResult::ReportLabel(ReportLabel::failed(&error))
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
