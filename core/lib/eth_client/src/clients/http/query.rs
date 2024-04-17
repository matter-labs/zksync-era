use std::sync::Arc;

use async_trait::async_trait;
use zksync_types::web3::{
    self,
    contract::Contract,
    ethabi, helpers,
    helpers::CallFuture,
    transports::Http,
    types::{
        Address, BlockId, BlockNumber, Bytes, Filter, Log, Transaction, TransactionId,
        TransactionReceipt, H256, U256, U64,
    },
    Transport, Web3,
};

use crate::{
    clients::http::{Method, COUNTERS, LATENCIES},
    types::{Error, ExecutedTxStatus, FailureInfo, FeeHistory, RawTokens},
    Block, ContractCall, EthInterface, RawTransactionBytes,
};

/// An "anonymous" Ethereum client that can invoke read-only methods that aren't
/// tied to a particular account.
#[derive(Debug, Clone)]
pub struct QueryClient {
    web3: Arc<Web3<Http>>,
}

impl From<Http> for QueryClient {
    fn from(transport: Http) -> Self {
        Self {
            web3: Arc::new(Web3::new(transport)),
        }
    }
}

impl QueryClient {
    /// Creates a new HTTP client.
    pub fn new(node_url: &str) -> Result<Self, Error> {
        let transport = Http::new(node_url)?;
        Ok(transport.into())
    }
}

#[async_trait]
impl EthInterface for QueryClient {
    async fn nonce_at_for_account(
        &self,
        account: Address,
        block: BlockNumber,
        component: &'static str,
    ) -> Result<U256, Error> {
        COUNTERS.call[&(Method::NonceAtForAccount, component)].inc();
        let latency = LATENCIES.direct[&Method::NonceAtForAccount].start();
        let nonce = self
            .web3
            .eth()
            .transaction_count(account, Some(block))
            .await?;
        latency.observe();
        Ok(nonce)
    }

    async fn block_number(&self, component: &'static str) -> Result<U64, Error> {
        COUNTERS.call[&(Method::BlockNumber, component)].inc();
        let latency = LATENCIES.direct[&Method::BlockNumber].start();
        let block_number = self.web3.eth().block_number().await?;
        latency.observe();
        Ok(block_number)
    }

    async fn get_gas_price(&self, component: &'static str) -> Result<U256, Error> {
        COUNTERS.call[&(Method::GetGasPrice, component)].inc();
        let latency = LATENCIES.direct[&Method::GetGasPrice].start();
        let network_gas_price = self.web3.eth().gas_price().await?;
        latency.observe();
        Ok(network_gas_price)
    }

    async fn send_raw_tx(&self, tx: RawTransactionBytes) -> Result<H256, Error> {
        let latency = LATENCIES.direct[&Method::SendRawTx].start();
        let tx = self.web3.eth().send_raw_transaction(Bytes(tx.0)).await?;
        latency.observe();
        Ok(tx)
    }

    async fn base_fee_history(
        &self,
        upto_block: usize,
        block_count: usize,
        component: &'static str,
    ) -> Result<Vec<u64>, Error> {
        const MAX_REQUEST_CHUNK: usize = 1024;

        COUNTERS.call[&(Method::BaseFeeHistory, component)].inc();
        let latency = LATENCIES.direct[&Method::BaseFeeHistory].start();
        let mut history = Vec::with_capacity(block_count);
        let from_block = upto_block.saturating_sub(block_count);

        // Here we are requesting `fee_history` from blocks
        // `(from_block; upto_block)` in chunks of size `MAX_REQUEST_CHUNK`
        // starting from the oldest block.
        for chunk_start in (from_block..=upto_block).step_by(MAX_REQUEST_CHUNK) {
            let chunk_end = (chunk_start + MAX_REQUEST_CHUNK).min(upto_block);
            let chunk_size = chunk_end - chunk_start;

            let block_count = helpers::serialize(&U256::from(chunk_size));
            let newest_block = helpers::serialize(&web3::types::BlockNumber::from(chunk_end));
            let reward_percentiles = helpers::serialize(&Option::<()>::None);

            let fee_history: FeeHistory = CallFuture::new(self.web3.transport().execute(
                "eth_feeHistory",
                vec![block_count, newest_block, reward_percentiles],
            ))
            .await?;
            if let Some(base_fees) = fee_history.base_fee_per_gas {
                history.extend(base_fees);
            }
        }

        latency.observe();
        Ok(history.into_iter().map(|fee| fee.as_u64()).collect())
    }

    async fn get_pending_block_base_fee_per_gas(
        &self,
        component: &'static str,
    ) -> Result<U256, Error> {
        COUNTERS.call[&(Method::PendingBlockBaseFee, component)].inc();
        let latency = LATENCIES.direct[&Method::PendingBlockBaseFee].start();

        let block = self
            .web3
            .eth()
            .block(BlockId::Number(BlockNumber::Pending))
            .await?;
        let block = if let Some(block) = block {
            block
        } else {
            // Fallback for local reth. Because of artificial nature of producing blocks in local reth setup
            // there may be no pending block
            self.web3
                .eth()
                .block(BlockId::Number(BlockNumber::Latest))
                .await?
                .expect("Latest block always exists")
        };

        latency.observe();
        // base_fee_per_gas always exists after London fork
        Ok(block.base_fee_per_gas.unwrap())
    }

    async fn get_tx_status(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<ExecutedTxStatus>, Error> {
        COUNTERS.call[&(Method::GetTxStatus, component)].inc();
        let latency = LATENCIES.direct[&Method::GetTxStatus].start();

        let receipt = self.tx_receipt(hash, component).await?;
        let res = receipt.and_then(|receipt| match receipt.status {
            Some(status) if receipt.block_number.is_some() => {
                let success = status.as_u64() == 1;

                Some(ExecutedTxStatus {
                    tx_hash: receipt.transaction_hash,
                    success,
                    receipt,
                })
            }
            _ => None,
        });

        latency.observe();
        Ok(res)
    }

    async fn failure_reason(&self, tx_hash: H256) -> Result<Option<FailureInfo>, Error> {
        let latency = LATENCIES.direct[&Method::FailureReason].start();
        let transaction = self.web3.eth().transaction(tx_hash.into()).await?;
        let receipt = self.web3.eth().transaction_receipt(tx_hash).await?;

        match (transaction, receipt) {
            (Some(transaction), Some(receipt)) => {
                let gas_limit = transaction.gas;
                let gas_used = receipt.gas_used;

                let call_request = web3::types::CallRequest {
                    from: transaction.from,
                    to: transaction.to,
                    gas: Some(transaction.gas),
                    gas_price: transaction.gas_price,
                    max_fee_per_gas: None,
                    max_priority_fee_per_gas: None,
                    value: Some(transaction.value),
                    data: Some(transaction.input),
                    transaction_type: None,
                    access_list: None,
                };

                let call_error = self
                    .web3
                    .eth()
                    .call(call_request, receipt.block_number.map(Into::into))
                    .await
                    .err();

                let failure_info = match call_error {
                    Some(web3::Error::Rpc(rpc_error)) => {
                        let revert_code = rpc_error.code.code();
                        let message_len = "execution reverted: ".len().min(rpc_error.message.len());
                        let revert_reason = rpc_error.message[message_len..].to_string();

                        Ok(Some(FailureInfo {
                            revert_code,
                            revert_reason,
                            gas_used,
                            gas_limit,
                        }))
                    }
                    Some(err) => Err(err.into()),
                    None => Ok(None),
                };

                latency.observe();
                failure_info
            }
            _ => Ok(None),
        }
    }

    async fn get_tx(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<Transaction>, Error> {
        COUNTERS.call[&(Method::GetTx, component)].inc();
        let tx = self
            .web3
            .eth()
            .transaction(TransactionId::Hash(hash))
            .await?;
        Ok(tx)
    }

    async fn call_contract_function(
        &self,
        call: ContractCall,
    ) -> Result<Vec<ethabi::Token>, Error> {
        let latency = LATENCIES.direct[&Method::CallContractFunction].start();
        let contract = Contract::new(self.web3.eth(), call.contract_address, call.contract_abi);
        let RawTokens(res) = contract
            .query(
                &call.inner.name,
                call.inner.params,
                call.inner.from,
                call.inner.options,
                call.inner.block,
            )
            .await?;
        latency.observe();
        Ok(res)
    }

    async fn tx_receipt(
        &self,
        tx_hash: H256,
        component: &'static str,
    ) -> Result<Option<TransactionReceipt>, Error> {
        COUNTERS.call[&(Method::TxReceipt, component)].inc();
        let latency = LATENCIES.direct[&Method::TxReceipt].start();
        let receipt = self.web3.eth().transaction_receipt(tx_hash).await?;
        latency.observe();
        Ok(receipt)
    }

    async fn eth_balance(&self, address: Address, component: &'static str) -> Result<U256, Error> {
        COUNTERS.call[&(Method::EthBalance, component)].inc();
        let latency = LATENCIES.direct[&Method::EthBalance].start();
        let balance = self.web3.eth().balance(address, None).await?;
        latency.observe();
        Ok(balance)
    }

    async fn logs(&self, filter: Filter, component: &'static str) -> Result<Vec<Log>, Error> {
        COUNTERS.call[&(Method::Logs, component)].inc();
        let latency = LATENCIES.direct[&Method::Logs].start();
        let logs = self.web3.eth().logs(filter).await?;
        latency.observe();
        Ok(logs)
    }

    async fn block(
        &self,
        block_id: BlockId,
        component: &'static str,
    ) -> Result<Option<Block<H256>>, Error> {
        COUNTERS.call[&(Method::Block, component)].inc();
        let latency = LATENCIES.direct[&Method::Block].start();
        // Copy of `web3::block` implementation. It's required to deserialize response as `crate::types::Block`
        // that has EIP-4844 fields.
        let block = {
            let include_txs = helpers::serialize(&false);

            let result = match block_id {
                BlockId::Hash(hash) => {
                    let hash = helpers::serialize(&hash);
                    self.web3
                        .transport()
                        .execute("eth_getBlockByHash", vec![hash, include_txs])
                }
                BlockId::Number(num) => {
                    let num = helpers::serialize(&num);
                    self.web3
                        .transport()
                        .execute("eth_getBlockByNumber", vec![num, include_txs])
                }
            };

            CallFuture::new(result).await?
        };
        latency.observe();
        Ok(block)
    }
}
