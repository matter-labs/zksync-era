use std::cmp::min;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use zksync_types::web3::{
    self,
    contract::{
        tokens::{Detokenize, Tokenize},
        Contract, Options,
    },
    ethabi,
    helpers::CallFuture,
    transports::Http,
    types::{
        Address, Block, BlockId, BlockNumber, Bytes, Filter, Log, Transaction, TransactionId,
        TransactionReceipt, H256, U256, U64,
    },
    Transport, Web3,
};

use crate::{
    types::{Error, ExecutedTxStatus, FailureInfo},
    EthInterface,
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
        let transport = web3::transports::Http::new(node_url)?;
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
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "nonce_at_for_account");
        let start = Instant::now();
        let nonce = self
            .web3
            .eth()
            .transaction_count(account, Some(block))
            .await?;
        metrics::histogram!("eth_client.direct.current_nonce", start.elapsed());
        Ok(nonce)
    }

    async fn block_number(&self, component: &'static str) -> Result<U64, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "block_number");
        let start = Instant::now();
        let block_number = self.web3.eth().block_number().await?;
        metrics::histogram!("eth_client.direct.block_number", start.elapsed());
        Ok(block_number)
    }

    async fn get_gas_price(&self, component: &'static str) -> Result<U256, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "get_gas_price");
        let start = Instant::now();
        let network_gas_price = self.web3.eth().gas_price().await?;
        metrics::histogram!("eth_client.direct.get_gas_price", start.elapsed());
        Ok(network_gas_price)
    }

    async fn send_raw_tx(&self, tx: Vec<u8>) -> Result<H256, Error> {
        let start = Instant::now();
        let tx = self.web3.eth().send_raw_transaction(Bytes(tx)).await?;
        metrics::histogram!("eth_client.direct.send_raw_tx", start.elapsed());
        Ok(tx)
    }

    async fn base_fee_history(
        &self,
        upto_block: usize,
        block_count: usize,
        component: &'static str,
    ) -> Result<Vec<u64>, Error> {
        const MAX_REQUEST_CHUNK: usize = 1024;

        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "base_fee_history");
        let start = Instant::now();

        let mut history = Vec::with_capacity(block_count);
        let from_block = upto_block.saturating_sub(block_count);

        // Here we are requesting fee_history from blocks
        // (from_block; upto_block] in chunks of size MAX_REQUEST_CHUNK
        // starting from the oldest block.
        for chunk_start in (from_block..=upto_block).step_by(MAX_REQUEST_CHUNK) {
            let chunk_end = (chunk_start + MAX_REQUEST_CHUNK).min(upto_block);
            let chunk_size = chunk_end - chunk_start;
            let chunk = self
                .web3
                .eth()
                .fee_history(chunk_size.into(), chunk_end.into(), None)
                .await?
                .base_fee_per_gas;

            history.extend(chunk);
        }

        metrics::histogram!("eth_client.direct.base_fee", start.elapsed());
        Ok(history.into_iter().map(|fee| fee.as_u64()).collect())
    }

    async fn get_pending_block_base_fee_per_gas(
        &self,
        component: &'static str,
    ) -> Result<U256, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "get_pending_block_base_fee_per_gas");
        let start = Instant::now();
        let block = self
            .web3
            .eth()
            .block(BlockId::Number(BlockNumber::Pending))
            .await?
            .expect("Pending block should always exist");

        metrics::histogram!("eth_client.direct.base_fee", start.elapsed());
        // base_fee_per_gas always exists after London fork
        Ok(block.base_fee_per_gas.unwrap())
    }

    async fn get_tx_status(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<ExecutedTxStatus>, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "get_tx_status");
        let start = Instant::now();

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

        metrics::histogram!("eth_client.direct.get_tx_status", start.elapsed());
        Ok(res)
    }

    async fn failure_reason(&self, tx_hash: H256) -> Result<Option<FailureInfo>, Error> {
        let start = Instant::now();
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
                        let message_len =
                            min("execution reverted: ".len(), rpc_error.message.len());
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

                metrics::histogram!("eth_client.direct.failure_reason", start.elapsed());

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
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "get_tx");
        let tx = self
            .web3
            .eth()
            .transaction(TransactionId::Hash(hash))
            .await?;
        Ok(tx)
    }

    #[allow(clippy::too_many_arguments)]
    async fn call_contract_function<R, A, B, P>(
        &self,
        func: &str,
        params: P,
        from: A,
        options: Options,
        block: B,
        contract_address: Address,
        contract_abi: ethabi::Contract,
    ) -> Result<R, Error>
    where
        R: Detokenize + Unpin,
        A: Into<Option<Address>> + Send,
        B: Into<Option<BlockId>> + Send,
        P: Tokenize + Send,
    {
        let start = Instant::now();
        let contract = Contract::new(self.web3.eth(), contract_address, contract_abi);
        let res = contract.query(func, params, from, options, block).await?;
        metrics::histogram!("eth_client.direct.call_contract_function", start.elapsed());
        Ok(res)
    }

    async fn tx_receipt(
        &self,
        tx_hash: H256,
        component: &'static str,
    ) -> Result<Option<TransactionReceipt>, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "tx_receipt");
        let start = Instant::now();
        let receipt = self.web3.eth().transaction_receipt(tx_hash).await?;
        metrics::histogram!("eth_client.direct.tx_receipt", start.elapsed());
        Ok(receipt)
    }

    async fn eth_balance(&self, address: Address, component: &'static str) -> Result<U256, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "eth_balance");
        let start = Instant::now();
        let balance = self.web3.eth().balance(address, None).await?;
        metrics::histogram!("eth_client.direct.eth_balance", start.elapsed());
        Ok(balance)
    }

    async fn logs(&self, filter: Filter, component: &'static str) -> Result<Vec<Log>, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "logs");
        let start = Instant::now();
        let logs = self.web3.eth().logs(filter).await?;
        metrics::histogram!("eth_client.direct.logs", start.elapsed());
        Ok(logs)
    }

    async fn block(
        &self,
        block_id: String,
        component: &'static str,
    ) -> Result<Option<Block<H256>>, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "block");
        let start = Instant::now();
        let block = CallFuture::new(
            self.web3
                .transport()
                .execute("eth_getBlockByNumber", vec![block_id.into(), false.into()]),
        )
        .await?;
        metrics::histogram!("eth_client.direct.block", start.elapsed());
        Ok(block)
    }
}
