use anyhow::format_err;
use num::BigUint;
use zksync_eth_client::clients::http_client::{Error, EthInterface};
use zksync_types::ethabi;
use zksync_types::web3::{
    contract::{tokens::Tokenize, Options},
    transports::Http,
    types::{TransactionReceipt, H256, U256, U64},
};
use zksync_types::L1ChainId;

use zksync_contracts::{erc20_contract, zksync_contract};
use zksync_eth_client::ETHDirectClient;
use zksync_eth_signer::PrivateKeySigner;
use zksync_types::aggregated_operations::{
    BlocksCommitOperation, BlocksExecuteOperation, BlocksProofOperation,
};
use zksync_types::{
    l1::{OpProcessingType, PriorityQueueType},
    tx::primitives::PackedEthSignature,
    Address,
};
use zksync_utils::{biguint_to_u256, u256_to_biguint};

const DEFAULT_PRIORITY_FEE: usize = 5; // 5 wei, doesn't really matter

/// Used to sign and post ETH transactions for the zkSync contracts.
#[derive(Debug, Clone)]
pub struct EthereumProvider {
    pub main_contract_eth_client: ETHDirectClient<PrivateKeySigner>,
    pub erc20_abi: ethabi::Contract,
    pub address: Address,
}

impl EthereumProvider {
    pub fn new(
        private_key: H256,
        transport: Http,
        contract_address: Address,
        chain_id: L1ChainId,
    ) -> Self {
        let erc20_abi = erc20_contract();
        let address = PackedEthSignature::address_from_private_key(&private_key)
            .expect("failed get address from private key");

        let eth_signer = PrivateKeySigner::new(private_key);
        let main_contract_eth_client = ETHDirectClient::new(
            transport,
            zksync_contract(),
            address,
            eth_signer,
            contract_address,
            DEFAULT_PRIORITY_FEE.into(),
            chain_id,
        );

        Self {
            main_contract_eth_client,
            erc20_abi,
            address,
        }
    }

    pub async fn eth_block_number(&self) -> Result<u64, Error> {
        self.main_contract_eth_client
            .block_number("provider")
            .await
            .map(|num| num.as_u64())
    }

    pub async fn eth_balance(&self, account_address: Option<Address>) -> Result<BigUint, Error> {
        let account_address = account_address.unwrap_or(self.address);
        self.main_contract_eth_client
            .eth_balance(account_address, "provider")
            .await
            .map(u256_to_biguint)
    }

    pub async fn get_layer_1_base_cost(
        &self,
        _queue_type: PriorityQueueType,
        _processing_type: OpProcessingType,
        _layer_2_tip_fee: BigUint,
    ) -> anyhow::Result<U256> {
        // let get_base_cost_func_name = match tx_id {
        //     TransactionID::Deposit => "depositBaseCost",
        //     TransactionID::AddToken => "addTokenBaseCost",
        //     TransactionID::Withdraw => "withdrawBaseCost",
        //     TransactionID::Execute => unimplemented!(),
        // };

        // let gas_price = self
        //     .main_contract_eth_client
        //     .get_gas_price("provider")
        //     .await?;

        // let layer_1_base_fee = self
        //     .main_contract_eth_client
        //     .call_main_contract_function(
        //         get_base_cost_func_name,
        //         (gas_price, queue_type as u8, processing_type as u8),
        //         None,
        //         default_tx_options(),
        //         None,
        //     )
        //     .await
        //     .map_err(|e| format_err!("Contract query fail: {}", e))?;

        // biguint_to_u256(layer_2_tip_fee)
        //     .checked_add(layer_1_base_fee)
        //     .ok_or_else(|| {
        //         format_err!("overflow when adding layer 1 base cost and layer 2 tip fee")
        //     })
    }

    pub async fn erc20_balance(
        &self,
        token_contract: &Address,
        account_address: Option<Address>,
    ) -> anyhow::Result<BigUint> {
        let account_address = account_address.unwrap_or(self.address);
        self.main_contract_eth_client
            .call_contract_function(
                "balanceOf",
                account_address,
                None,
                Options::default(),
                None,
                *token_contract,
                erc20_contract(),
            )
            .await
            .map(u256_to_biguint)
            .map_err(|e| format_err!("Contract query fail: {}", e))
    }

    pub async fn balances_to_withdraw(
        &self,
        token_address: Address,
        account_address: Option<Address>,
    ) -> anyhow::Result<BigUint> {
        let account_address = account_address.unwrap_or(self.address);
        self.main_contract_eth_client
            .call_main_contract_function(
                "getPendingBalance",
                (account_address, token_address),
                None,
                default_tx_options(),
                None,
            )
            .await
            .map(u256_to_biguint)
            .map_err(|e| format_err!("Contract query fail: {}", e))
    }

    pub async fn total_blocks_committed(&self) -> anyhow::Result<u64> {
        self.main_contract_eth_client
            .call_main_contract_function(
                "getTotalBlocksCommitted",
                (),
                None,
                default_tx_options(),
                None,
            )
            .await
            .map_err(|e| format_err!("Contract query fail: {}", e))
    }

    pub async fn total_blocks_verified(&self) -> anyhow::Result<u64> {
        self.main_contract_eth_client
            .call_main_contract_function(
                "getTotalBlocksVerified",
                (),
                None,
                default_tx_options(),
                None,
            )
            .await
            .map_err(|e| format_err!("Contract query fail: {}", e))
    }

    pub async fn total_blocks_executed(&self) -> anyhow::Result<u64> {
        self.main_contract_eth_client
            .call_main_contract_function(
                "getTotalBlocksExecuted",
                (),
                None,
                default_tx_options(),
                None,
            )
            .await
            .map_err(|e| format_err!("Contract query fail: {}", e))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_custom_token(
        &self,
        _token_address: Address,
        _name: String,
        _symbol: String,
        _decimals: u8,
        _queue_type: PriorityQueueType,
        _processing_type: OpProcessingType,
        _layer_2_tip_fee: BigUint,
    ) -> anyhow::Result<EthExecResult> {
        // let value = self
        //     .get_layer_1_base_cost(
        //         TransactionID::AddToken,
        //         queue_type,
        //         processing_type,
        //         layer_2_tip_fee,
        //     )
        //     .await?;

        // let data = self.main_contract_eth_client.encode_tx_data(
        //     "addCustomToken",
        //     (
        //         token_address,
        //         name,
        //         symbol,
        //         decimals,
        //         queue_type as u8,
        //         processing_type as u8,
        //     ),
        // );

        // let signed_tx = self
        //     .main_contract_eth_client
        //     .sign_prepared_tx(
        //         data,
        //         Options::with(|opt| {
        //             opt.value = Some(value);
        //             opt.gas = Some(500_000.into());
        //         }),
        //         "provider",
        //     )
        //     .await
        //     .map_err(|e| format_err!("Add token send err: {}", e))?;

        // let receipt =
        //     send_raw_tx_wait_confirmation(&self.main_contract_eth_client, signed_tx.raw_tx).await?;

        // Ok(EthExecResult::new(receipt, &self.main_contract_eth_client).await)
    }

    pub async fn add_token(
        &self,
        _token_address: Address,
        _queue_type: PriorityQueueType,
        _processing_type: OpProcessingType,
        _layer_2_tip_fee: BigUint,
    ) -> anyhow::Result<EthExecResult> {
        // let value = self
        //     .get_layer_1_base_cost(
        //         TransactionID::AddToken,
        //         queue_type,
        //         processing_type,
        //         layer_2_tip_fee,
        //     )
        //     .await?;

        // let data = self.main_contract_eth_client.encode_tx_data(
        //     "addToken",
        //     (token_address, queue_type as u8, processing_type as u8),
        // );

        // let signed_tx = self
        //     .main_contract_eth_client
        //     .sign_prepared_tx(
        //         data,
        //         Options::with(|opt| {
        //             opt.value = Some(value);
        //             opt.gas = Some(500_000.into());
        //         }),
        //         "provider",
        //     )
        //     .await
        //     .map_err(|e| format_err!("Add token send err: {}", e))?;

        // let receipt =
        //     send_raw_tx_wait_confirmation(&self.main_contract_eth_client, signed_tx.raw_tx).await?;

        // Ok(EthExecResult::new(receipt, &self.main_contract_eth_client).await)
    }

    pub async fn request_withdraw(
        &self,
        _token: Address,
        _amount: BigUint,
        _to: Address,
        _queue_type: PriorityQueueType,
        _processing_type: OpProcessingType,
        _layer_2_tip_fee: BigUint,
    ) -> anyhow::Result<EthExecResult> {
        // let value = self
        //     .get_layer_1_base_cost(
        //         TransactionID::Withdraw,
        //         queue_type,
        //         processing_type,
        //         layer_2_tip_fee,
        //     )
        //     .await?;

        // let data = self.main_contract_eth_client.encode_tx_data(
        //     "requestWithdraw",
        //     (
        //         token,
        //         biguint_to_u256(amount),
        //         to,
        //         queue_type as u8,
        //         processing_type as u8,
        //     ),
        // );

        // let signed_tx = self
        //     .main_contract_eth_client
        //     .sign_prepared_tx(
        //         data,
        //         Options::with(|opt| {
        //             opt.value = Some(value);
        //             opt.gas = Some(500_000.into());
        //         }),
        //         "provider",
        //     )
        //     .await
        //     .map_err(|e| format_err!("Request withdraw send err: {}", e))?;

        // let receipt =
        //     send_raw_tx_wait_confirmation(&self.main_contract_eth_client, signed_tx.raw_tx).await?;

        // Ok(EthExecResult::new(receipt, &self.main_contract_eth_client).await)
    }

    pub async fn deposit_eth(
        &self,
        _amount: BigUint,
        _to: &Address,
        _queue_type: PriorityQueueType,
        _processing_type: OpProcessingType,
        _layer_2_tip_fee: BigUint,
    ) -> anyhow::Result<EthExecResult> {
        // let value = self
        //     .get_layer_1_base_cost(
        //         TransactionID::Deposit,
        //         queue_type,
        //         processing_type,
        //         layer_2_tip_fee + &amount,
        //     )
        //     .await?;

        // let data = self.main_contract_eth_client.encode_tx_data(
        //     "depositETH",
        //     (
        //         biguint_to_u256(amount),
        //         *to,
        //         queue_type as u8,
        //         processing_type as u8,
        //     ),
        // );

        // let signed_tx = self
        //     .main_contract_eth_client
        //     .sign_prepared_tx(
        //         data,
        //         Options::with(|opt| {
        //             opt.value = Some(value);
        //             opt.gas = Some(500_000.into());
        //         }),
        //         "provider",
        //     )
        //     .await
        //     .map_err(|e| format_err!("Deposit eth send err: {}", e))?;

        // let receipt =
        //     send_raw_tx_wait_confirmation(&self.main_contract_eth_client, signed_tx.raw_tx).await?;

        // Ok(EthExecResult::new(receipt, &self.main_contract_eth_client).await)
    }

    pub async fn send_eth(&self, to: Address, value: BigUint) -> anyhow::Result<EthExecResult> {
        let signed_tx = self
            .main_contract_eth_client
            .sign_prepared_tx_for_addr(
                Vec::new(),
                to,
                Options::with(|opt| {
                    opt.value = Some(biguint_to_u256(value));
                    opt.gas = Some(500_000.into());
                }),
                "provider",
            )
            .await
            .map_err(|e| format_err!("Send err: {}", e))?;

        let receipt =
            send_raw_tx_wait_confirmation(&self.main_contract_eth_client, signed_tx.raw_tx).await?;

        Ok(EthExecResult::new(receipt, &self.main_contract_eth_client).await)
    }

    pub async fn allowance(&self, token_address: Address) -> Result<U256, Error> {
        self.main_contract_eth_client
            .allowance(token_address, self.erc20_abi.clone())
            .await
    }

    pub async fn approve_erc20(
        &self,
        token_address: Address,
        amount: BigUint,
    ) -> anyhow::Result<EthExecResult> {
        let contract_function = self
            .erc20_abi
            .function("approve")
            .expect("failed to get function parameters");
        let params = (
            self.main_contract_eth_client.contract_addr(),
            biguint_to_u256(amount),
        );
        let data = contract_function
            .encode_input(&params.into_tokens())
            .expect("failed to encode parameters");

        let signed_tx = self
            .main_contract_eth_client
            .sign_prepared_tx_for_addr(data, token_address, default_tx_options(), "provider")
            .await
            .map_err(|e| format_err!("Approve send err: {}", e))?;
        let receipt =
            send_raw_tx_wait_confirmation(&self.main_contract_eth_client, signed_tx.raw_tx).await?;

        Ok(EthExecResult::new(receipt, &self.main_contract_eth_client).await)
    }

    pub async fn deposit_erc20(
        &self,
        _token_contract: Address,
        _amount: BigUint,
        _to: &Address,
        _queue_type: PriorityQueueType,
        _processing_type: OpProcessingType,
        _layer_2_tip_fee: BigUint,
    ) -> anyhow::Result<EthExecResult> {
        // let value = self
        //     .get_layer_1_base_cost(
        //         TransactionID::Deposit,
        //         queue_type,
        //         processing_type,
        //         layer_2_tip_fee,
        //     )
        //     .await?;

        // let data = self.main_contract_eth_client.encode_tx_data(
        //     "depositERC20",
        //     (
        //         token_contract,
        //         biguint_to_u256(amount.clone()),
        //         *to,
        //         queue_type as u8,
        //         processing_type as u8,
        //     ),
        // );
        // let signed_tx = self
        //     .main_contract_eth_client
        //     .sign_prepared_tx(
        //         data,
        //         Options::with(|opt| {
        //             opt.value = Some(value);
        //             opt.gas = Some(500_000.into());
        //         }),
        //         "provider",
        //     )
        //     .await
        //     .map_err(|e| format_err!("Deposit erc20 send err: {}", e))?;

        // let receipt =
        //     send_raw_tx_wait_confirmation(&self.main_contract_eth_client, signed_tx.raw_tx).await?;

        // Ok(EthExecResult::new(receipt, &self.main_contract_eth_client).await)
    }

    pub async fn commit_blocks(
        &self,
        commit_operation: &BlocksCommitOperation,
    ) -> anyhow::Result<EthExecResult> {
        let data = self.main_contract_eth_client.encode_tx_data(
            "commitBlocks",
            commit_operation.get_eth_tx_args().as_slice(),
        );
        let signed_tx = self
            .main_contract_eth_client
            .sign_prepared_tx(
                data,
                Options::with(|f| f.gas = Some(U256::from(9 * 10u64.pow(6)))),
                "provider",
            )
            .await
            .map_err(|e| format_err!("Commit block send err: {}", e))?;

        let receipt =
            send_raw_tx_wait_confirmation(&self.main_contract_eth_client, signed_tx.raw_tx).await?;

        Ok(EthExecResult::new(receipt, &self.main_contract_eth_client).await)
    }

    // Verifies block using provided proof or empty proof if None is provided. (`DUMMY_VERIFIER` should be enabled on the contract).
    pub async fn verify_blocks(
        &self,
        proof_operation: &BlocksProofOperation,
    ) -> anyhow::Result<EthExecResult> {
        let data = self
            .main_contract_eth_client
            .encode_tx_data("proveBlocks", proof_operation.get_eth_tx_args().as_slice());
        let signed_tx = self
            .main_contract_eth_client
            .sign_prepared_tx(
                data,
                Options::with(|f| f.gas = Some(U256::from(10 * 10u64.pow(6)))),
                "provider",
            )
            .await
            .map_err(|e| format_err!("Verify block send err: {}", e))?;

        let receipt =
            send_raw_tx_wait_confirmation(&self.main_contract_eth_client, signed_tx.raw_tx).await?;

        Ok(EthExecResult::new(receipt, &self.main_contract_eth_client).await)
    }

    pub async fn execute_blocks(
        &self,
        execute_operation: &BlocksExecuteOperation,
    ) -> anyhow::Result<EthExecResult> {
        let data = self.main_contract_eth_client.encode_tx_data(
            "executeBlocks",
            execute_operation.get_eth_tx_args().as_slice(),
        );
        let signed_tx = self
            .main_contract_eth_client
            .sign_prepared_tx(
                data,
                Options::with(|f| f.gas = Some(U256::from(9 * 10u64.pow(6)))),
                "provider",
            )
            .await
            .map_err(|e| format_err!("Execute block send err: {}", e))?;

        let receipt =
            send_raw_tx_wait_confirmation(&self.main_contract_eth_client, signed_tx.raw_tx).await?;

        Ok(EthExecResult::new(receipt, &self.main_contract_eth_client).await)
    }

    pub async fn revert_blocks(&self, last_committed_block: u32) -> anyhow::Result<EthExecResult> {
        let data = self
            .main_contract_eth_client
            .encode_tx_data("revertBlocks", last_committed_block);
        let signed_tx = self
            .main_contract_eth_client
            .sign_prepared_tx(
                data,
                Options::with(|f| f.gas = Some(U256::from(9 * 10u64.pow(6)))),
                "provider",
            )
            .await
            .map_err(|e| format_err!("Revert blocks send err: {}", e))?;

        let receipt =
            send_raw_tx_wait_confirmation(&self.main_contract_eth_client, signed_tx.raw_tx).await?;

        Ok(EthExecResult::new(receipt, &self.main_contract_eth_client).await)
    }
}

#[derive(Debug, Clone)]
pub struct EthExecResult {
    receipt: TransactionReceipt,
    revert_reason: Option<String>,
}

impl EthExecResult {
    pub async fn new(
        receipt: TransactionReceipt,
        client: &ETHDirectClient<PrivateKeySigner>,
    ) -> Self {
        let revert_reason = if receipt.status == Some(U64::from(1)) {
            None
        } else {
            let reason = client
                .failure_reason(receipt.transaction_hash)
                .await
                .expect("Failed to get revert reason")
                .unwrap()
                .revert_reason;

            Some(reason)
        };

        Self {
            receipt,
            revert_reason,
        }
    }

    pub fn expect_success(self) -> TransactionReceipt {
        assert!(
            self.revert_reason.is_none(),
            "Expected transaction success: revert_reason: {}, tx: 0x{:x}",
            self.revert_reason.unwrap(),
            self.receipt.transaction_hash
        );

        self.receipt
    }

    pub fn expect_reverted(self, code: &str) -> TransactionReceipt {
        if let Some(revert_reason) = self.revert_reason {
            assert_eq!(
                revert_reason,
                code,
                "Transaction reverted with incorrect revert reason expected: {:?}, found: {:?}, tx: 0x{:x}",
                code,
                revert_reason,
                self.receipt.transaction_hash
            );
        } else {
            panic!(
                "Expected transaction reverted: expected code: {:?}, tx: 0x{:x}",
                code, self.receipt.transaction_hash
            );
        }

        self.receipt
    }
}

async fn send_raw_tx_wait_confirmation(
    client: &ETHDirectClient<PrivateKeySigner>,
    raw_tx: Vec<u8>,
) -> Result<TransactionReceipt, anyhow::Error> {
    let tx_hash = client
        .send_raw_tx(raw_tx)
        .await
        .map_err(|e| format_err!("Failed to send raw tx: {}", e))?;
    loop {
        if let Some(receipt) = client
            .tx_receipt(tx_hash, "provider")
            .await
            .map_err(|e| format_err!("Failed to get receipt from eth node: {}", e))?
        {
            return Ok(receipt);
        }
    }
}

fn default_tx_options() -> Options {
    // Set the gas limit, so `eth_client` won't complain about it.
    Options {
        gas: Some(500_000.into()),
        ..Default::default()
    }
}
