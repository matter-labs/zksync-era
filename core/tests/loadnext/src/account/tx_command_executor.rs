use std::time::Instant;

use zksync_types::{
    api::{BlockNumber, TransactionReceipt},
    l2::L2Tx,
    Address, H256, MAX_L1_TRANSACTION_GAS_LIMIT, U256,
};

use crate::{
    account::{AccountLifespan, ExecutionType},
    command::{IncorrectnessModifier, TxCommand, TxType},
    constants::{
        ETH_CONFIRMATION_TIMEOUT, ETH_POLLING_INTERVAL, MIN_ALLOWANCE_FOR_PAYMASTER_ESTIMATE,
    },
    corrupted_tx::Corrupted,
    report::ReportLabel,
    sdk::{
        error::ClientError,
        ethabi,
        ethereum::PriorityOpHolder,
        utils::{
            get_approval_based_paymaster_input, get_approval_based_paymaster_input_for_estimation,
        },
        EthNamespaceClient,
    },
    utils::format_gwei,
};

#[derive(Debug)]
pub enum SubmitResult {
    TxHash(H256),
    ReportLabel(ReportLabel),
}

impl AccountLifespan {
    pub(super) async fn execute_tx_command(
        &mut self,
        command: &TxCommand,
    ) -> Result<SubmitResult, ClientError> {
        match command.command_type {
            TxType::WithdrawToOther | TxType::WithdrawToSelf => {
                self.execute_withdraw(command).await
            }
            TxType::Deposit => self.execute_deposit(command).await,
            TxType::DeployContract => self.execute_deploy_contract(command).await,
            TxType::L2Execute => {
                self.execute_loadnext_contract(command, ExecutionType::L2)
                    .await
            }
            TxType::L1Execute => {
                self.execute_loadnext_contract(command, ExecutionType::L1)
                    .await
            }
        }
    }

    fn tx_creation_error(err: ClientError) -> ClientError {
        // Translate network errors (so operation will be retried), but don't accept other ones.
        // For example, we will retry operation if fee ticker returned an error,
        // but will panic if transaction cannot be signed.
        match err {
            ClientError::NetworkError(_)
            | ClientError::RpcError(_)
            | ClientError::MalformedResponse(_) => err,
            _ => panic!("Transaction should be correct"),
        }
    }

    async fn apply_modifier(&self, tx: L2Tx, modifier: IncorrectnessModifier) -> L2Tx {
        let wallet = &self.wallet.wallet;
        tx.apply_modifier(modifier, &wallet.signer).await
    }

    /// Returns the balances for ETH and the main token on the L1.
    /// This function is used to check whether the L1 operation can be performed or should be
    /// skipped.
    async fn l1_balances(&self) -> Result<(U256, U256), ClientError> {
        let wallet = &self.wallet.wallet;
        let ethereum = wallet.ethereum(&self.config.l1_rpc_address).await?;
        let eth_balance = ethereum.balance().await?;
        let erc20_balance = ethereum
            .erc20_balance(wallet.address(), self.main_l1_token)
            .await?;

        Ok((eth_balance, erc20_balance))
    }

    async fn execute_deposit(&self, command: &TxCommand) -> Result<SubmitResult, ClientError> {
        let wallet = &self.wallet.wallet;

        let (eth_balance, erc20_balance) = self.l1_balances().await?;
        if eth_balance.is_zero() || erc20_balance < command.amount {
            // We don't have either funds in L1 to pay for tx or to deposit.
            // It's not a problem with the server, thus we mark this operation as skipped.
            let label = ReportLabel::skipped("No L1 balance");
            return Ok(SubmitResult::ReportLabel(label));
        }

        let mut ethereum = wallet.ethereum(&self.config.l1_rpc_address).await?;
        ethereum.set_confirmation_timeout(ETH_CONFIRMATION_TIMEOUT);
        ethereum.set_polling_interval(ETH_POLLING_INTERVAL);
        let gas_price = ethereum
            .client()
            .as_ref()
            .get_gas_price()
            .await
            .map_err(|_| ClientError::Other)?;

        // We should check whether we've previously approved ERC-20 deposits.
        let deposits_allowed = ethereum
            .is_erc20_deposit_approved(self.main_l1_token, None)
            .await?;
        if !deposits_allowed {
            let approve_tx_hash = ethereum
                .approve_erc20_token_deposits(self.main_l1_token, None)
                .await?;
            // Before submitting the deposit, wait for the approve transaction confirmation.
            match ethereum.wait_for_tx(approve_tx_hash).await {
                Ok(receipt) => {
                    if receipt.status != Some(1.into()) {
                        let label = ReportLabel::skipped("Approve transaction failed");
                        return Ok(SubmitResult::ReportLabel(label));
                    }
                }
                Err(_) => {
                    let label = ReportLabel::skipped("Approve transaction failed");
                    return Ok(SubmitResult::ReportLabel(label));
                }
            }
        }

        let eth_balance = ethereum.balance().await?;
        if eth_balance < gas_price * U256::from(MAX_L1_TRANSACTION_GAS_LIMIT) {
            // We don't have either funds in L1 to pay for tx or to deposit.
            // It's not a problem with the server, thus we mark this operation as skipped.
            let label = ReportLabel::skipped("Not enough L1 balance");
            return Ok(SubmitResult::ReportLabel(label));
        }

        let response = ethereum
            .deposit(
                self.main_l1_token,
                command.amount,
                wallet.address(),
                None,
                None,
                None,
            )
            .await;
        let eth_tx_hash = match response {
            Ok(hash) => hash,
            Err(err) => {
                // Most likely we don't have enough ETH to perform operations.
                // Just mark the operations as skipped.
                let reason = format!("Unable to perform an L1 operation. Reason: {err}");
                return Ok(SubmitResult::ReportLabel(ReportLabel::skipped(reason)));
            }
        };

        self.get_priority_op_l2_hash(eth_tx_hash).await
    }

    async fn get_priority_op_l2_hash(
        &self,
        eth_tx_hash: H256,
    ) -> Result<SubmitResult, ClientError> {
        let wallet = &self.wallet.wallet;

        let mut ethereum = wallet.ethereum(&self.config.l1_rpc_address).await?;
        ethereum.set_confirmation_timeout(ETH_CONFIRMATION_TIMEOUT);
        ethereum.set_polling_interval(ETH_POLLING_INTERVAL);

        let receipt = ethereum.wait_for_tx(eth_tx_hash).await?;

        match receipt.priority_op() {
            Some(tx_common_data) => Ok(SubmitResult::TxHash(tx_common_data.canonical_tx_hash)),
            None => {
                // Probably we did something wrong, no big deal.
                let label = ReportLabel::skipped("Ethereum transaction for deposit failed");
                Ok(SubmitResult::ReportLabel(label))
            }
        }
    }

    async fn execute_submit(
        &mut self,
        tx: L2Tx,
        modifier: IncorrectnessModifier,
    ) -> Result<SubmitResult, ClientError> {
        let nonce = tx.nonce();
        let result = match modifier {
            IncorrectnessModifier::IncorrectSignature => {
                let wallet = self.wallet.corrupted_wallet.clone();
                self.submit(modifier, wallet.send_transaction(tx).await)
                    .await
            }
            _ => {
                let wallet = self.wallet.wallet.clone();
                self.submit(modifier, wallet.send_transaction(tx).await)
                    .await
            }
        }?;

        // Update current nonce for future txs
        // If the transaction has a `tx_hash` and is small enough to be included in a block, this tx will change the nonce.
        // We can be sure that the nonce will be changed based on this assumption.
        if let SubmitResult::TxHash(_) = &result {
            self.current_nonce = Some(nonce + 1)
        }

        Ok(result)
    }

    async fn execute_withdraw(&mut self, command: &TxCommand) -> Result<SubmitResult, ClientError> {
        let tx = self.build_withdraw(command).await?;
        self.execute_submit(tx, command.modifier).await
    }

    pub(super) async fn build_withdraw(&self, command: &TxCommand) -> Result<L2Tx, ClientError> {
        let wallet = self.wallet.wallet.clone();

        let mut builder = wallet
            .start_withdraw()
            .to(command.to)
            .amount(command.amount)
            .token(self.main_l2_token);

        let fee = builder
            .estimate_fee(Some(get_approval_based_paymaster_input_for_estimation(
                self.paymaster_address,
                self.main_l2_token,
                MIN_ALLOWANCE_FOR_PAYMASTER_ESTIMATE.into(),
            )))
            .await?;
        builder = builder.fee(fee.clone());

        let paymaster_params = get_approval_based_paymaster_input(
            self.paymaster_address,
            self.main_l2_token,
            fee.max_total_fee(),
            Vec::new(),
        );
        builder = builder.fee(fee);
        builder = builder.paymaster_params(paymaster_params);

        if let Some(nonce) = self.current_nonce {
            builder = builder.nonce(nonce);
        }

        let tx = builder.tx().await.map_err(Self::tx_creation_error)?;

        Ok(self.apply_modifier(tx, command.modifier).await)
    }

    async fn execute_deploy_contract(
        &mut self,
        command: &TxCommand,
    ) -> Result<SubmitResult, ClientError> {
        let tx = self.build_deploy_loadnext_contract(command).await?;
        self.execute_submit(tx, command.modifier).await
    }

    async fn build_deploy_loadnext_contract(
        &self,
        command: &TxCommand,
    ) -> Result<L2Tx, ClientError> {
        let wallet = self.wallet.wallet.clone();
        let constructor_calldata = ethabi::encode(&[ethabi::Token::Uint(U256::from(
            self.contract_execution_params.reads,
        ))]);

        let mut builder = wallet
            .start_deploy_contract()
            .bytecode(self.wallet.test_contract.bytecode.to_vec())
            .constructor_calldata(constructor_calldata);

        let fee = builder
            .estimate_fee(Some(get_approval_based_paymaster_input_for_estimation(
                self.paymaster_address,
                self.main_l2_token,
                MIN_ALLOWANCE_FOR_PAYMASTER_ESTIMATE.into(),
            )))
            .await?;
        builder = builder.fee(fee.clone());

        let paymaster_params = get_approval_based_paymaster_input(
            self.paymaster_address,
            self.main_l2_token,
            fee.max_total_fee(),
            Vec::new(),
        );
        builder = builder.fee(fee);
        builder = builder.paymaster_params(paymaster_params);

        if let Some(nonce) = self.current_nonce {
            builder = builder.nonce(nonce);
        }

        let tx = builder.tx().await.map_err(Self::tx_creation_error)?;

        Ok(self.apply_modifier(tx, command.modifier).await)
    }

    async fn execute_loadnext_contract(
        &mut self,
        command: &TxCommand,
        execution_type: ExecutionType,
    ) -> Result<SubmitResult, ClientError> {
        const L1_TRANSACTION_GAS_LIMIT: u32 = 5_000_000;

        let Some(&contract_address) = self.wallet.deployed_contract_address.get() else {
            let label =
                ReportLabel::skipped("Account haven't successfully deployed a contract yet");
            return Ok(SubmitResult::ReportLabel(label));
        };

        match execution_type {
            ExecutionType::L1 => {
                let calldata = self.prepare_calldata_for_loadnext_contract();
                let ethereum = self
                    .wallet
                    .wallet
                    .ethereum(&self.config.l1_rpc_address)
                    .await?;
                let response = ethereum
                    .request_execute(
                        contract_address,
                        U256::zero(),
                        calldata,
                        L1_TRANSACTION_GAS_LIMIT.into(),
                        Some(self.wallet.test_contract.factory_deps()),
                        None,
                        None,
                        Default::default(),
                    )
                    .await;

                let tx_hash = match response {
                    Ok(hash) => hash,
                    Err(ClientError::NetworkError(err)) if err.contains("insufficient funds") => {
                        let reason =
                            format!("L1 execution tx failed because of insufficient funds: {err}");
                        let label = ReportLabel::skipped(reason);
                        return Ok(SubmitResult::ReportLabel(label));
                    }
                    Err(err) => {
                        let label = ReportLabel::failed(err.to_string());
                        return Ok(SubmitResult::ReportLabel(label));
                    }
                };
                self.get_priority_op_l2_hash(tx_hash).await
            }

            ExecutionType::L2 => {
                let mut started_at = Instant::now();
                let tx = self
                    .build_execute_loadnext_contract(command, contract_address)
                    .await?;
                tracing::trace!(
                    "Account {:?}: execute_loadnext_contract: tx built in {:?}",
                    self.wallet.wallet.address(),
                    started_at.elapsed()
                );
                started_at = Instant::now();
                let result = self.execute_submit(tx, command.modifier).await;
                tracing::trace!(
                    "Account {:?}: execute_loadnext_contract: tx executed in {:?}",
                    self.wallet.wallet.address(),
                    started_at.elapsed()
                );
                result
            }
        }
    }

    fn prepare_calldata_for_loadnext_contract(&self) -> Vec<u8> {
        let contract = &self.wallet.test_contract.abi;
        let function = contract.function("execute").unwrap();
        function
            .encode_input(&vec![
                ethabi::Token::Uint(U256::from(self.contract_execution_params.reads)),
                ethabi::Token::Uint(U256::from(self.contract_execution_params.initial_writes)),
                ethabi::Token::Uint(U256::from(self.contract_execution_params.repeated_writes)),
                ethabi::Token::Uint(U256::from(self.contract_execution_params.hashes)),
                ethabi::Token::Uint(U256::from(self.contract_execution_params.events)),
                ethabi::Token::Uint(U256::from(self.contract_execution_params.recursive_calls)),
                ethabi::Token::Uint(U256::from(self.contract_execution_params.deploys)),
            ])
            .expect("failed to encode parameters when creating calldata")
    }

    async fn build_execute_loadnext_contract(
        &mut self,
        command: &TxCommand,
        contract_address: Address,
    ) -> Result<L2Tx, ClientError> {
        let wallet = &self.wallet.wallet;

        let calldata = self.prepare_calldata_for_loadnext_contract();
        let mut builder = wallet
            .start_execute_contract()
            .calldata(calldata)
            .contract_address(contract_address)
            .factory_deps(self.wallet.test_contract.factory_deps());

        let fee = builder
            .estimate_fee(Some(get_approval_based_paymaster_input_for_estimation(
                self.paymaster_address,
                self.main_l2_token,
                MIN_ALLOWANCE_FOR_PAYMASTER_ESTIMATE.into(),
            )))
            .await?;
        tracing::trace!(
            "Account {:?}: fee estimated. Max total fee: {}, gas limit: {}gas; Max gas price: {}WEI, \
             Gas per pubdata: {:?}gas",
            self.wallet.wallet.address(),
            format_gwei(fee.max_total_fee()),
            fee.gas_limit,
            fee.max_fee_per_gas,
            fee.gas_per_pubdata_limit
        );
        builder = builder.fee(fee.clone());

        let paymaster_params = get_approval_based_paymaster_input(
            self.paymaster_address,
            self.main_l2_token,
            fee.max_total_fee(),
            Vec::new(),
        );
        builder = builder.fee(fee);
        builder = builder.paymaster_params(paymaster_params);

        if let Some(nonce) = self.current_nonce {
            builder = builder.nonce(nonce);
        }

        let tx = builder.tx().await.map_err(Self::tx_creation_error)?;

        Ok(self.apply_modifier(tx, command.modifier).await)
    }

    pub(crate) async fn get_tx_receipt_for_committed_block(
        &mut self,
        tx_hash: H256,
    ) -> Result<Option<TransactionReceipt>, ClientError> {
        let response = self
            .wallet
            .wallet
            .provider
            .get_transaction_receipt(tx_hash)
            .await?;

        let Some(receipt) = response else {
            return Ok(None);
        };

        let block_number = receipt.block_number;

        let response = self
            .wallet
            .wallet
            .provider
            .get_block_by_number(BlockNumber::Committed, false)
            .await?;
        if let Some(received_number) = response.map(|block| block.number) {
            if block_number <= received_number {
                return Ok(Some(receipt));
            }
        }
        Ok(None)
    }
}
