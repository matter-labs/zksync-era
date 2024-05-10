use std::fmt;

use async_trait::async_trait;
use jsonrpsee::core::ClientError;
use zksync_types::{web3, Address, L1ChainId, H256, U256, U64};
use zksync_web3_decl::{
    client::{TaggedClient, L1},
    error::{ClientRpcContext, EnrichedClientError},
};

use super::{decl::L1EthNamespaceClient, Method, COUNTERS, LATENCIES};
use crate::{
    types::{Error, ExecutedTxStatus, FailureInfo},
    EthInterface, RawTransactionBytes,
};

#[async_trait]
impl<T> EthInterface for T
where
    T: TaggedClient<Net = L1> + L1EthNamespaceClient + Clone + fmt::Debug + Send + Sync + 'static,
{
    fn clone_boxed(&self) -> Box<dyn EthInterface> {
        Box::new(self.clone())
    }

    fn for_component(mut self: Box<Self>, component_name: &'static str) -> Box<dyn EthInterface> {
        self.set_component(component_name);
        self
    }

    async fn fetch_chain_id(&self) -> Result<L1ChainId, Error> {
        COUNTERS.call[&(Method::ChainId, self.component())].inc();
        let latency = LATENCIES.direct[&Method::ChainId].start();
        let raw_chain_id = self.chain_id().rpc_context("chain_id").await?;
        latency.observe();
        let chain_id = u64::try_from(raw_chain_id).map_err(|err| {
            let err = ClientError::Custom(format!("invalid chainId: {err}"));
            let err = EnrichedClientError::new(err, "chain_id").with_arg("chain_id", &raw_chain_id);
            Error::EthereumGateway(err)
        })?;
        Ok(L1ChainId(chain_id))
    }

    async fn nonce_at_for_account(
        &self,
        account: Address,
        block: web3::BlockNumber,
    ) -> Result<U256, Error> {
        COUNTERS.call[&(Method::NonceAtForAccount, self.component())].inc();
        let latency = LATENCIES.direct[&Method::NonceAtForAccount].start();
        let nonce = self
            .get_transaction_count(account, block)
            .rpc_context("get_transaction_count")
            .with_arg("account", &account)
            .with_arg("block", &block)
            .await?;
        latency.observe();
        Ok(nonce)
    }

    async fn block_number(&self) -> Result<U64, Error> {
        COUNTERS.call[&(Method::BlockNumber, self.component())].inc();
        let latency = LATENCIES.direct[&Method::BlockNumber].start();
        let block_number = self
            .get_block_number()
            .rpc_context("get_block_number")
            .await?;
        latency.observe();
        Ok(block_number)
    }

    async fn get_gas_price(&self) -> Result<U256, Error> {
        COUNTERS.call[&(Method::GetGasPrice, self.component())].inc();
        let latency = LATENCIES.direct[&Method::GetGasPrice].start();
        let network_gas_price = self.gas_price().rpc_context("gas_price").await?;
        latency.observe();
        Ok(network_gas_price)
    }

    async fn send_raw_tx(&self, tx: RawTransactionBytes) -> Result<H256, Error> {
        let latency = LATENCIES.direct[&Method::SendRawTx].start();
        let tx = self
            .send_raw_transaction(web3::Bytes(tx.0))
            .rpc_context("send_raw_transaction")
            .await?;
        latency.observe();
        Ok(tx)
    }

    async fn base_fee_history(
        &self,
        upto_block: usize,
        block_count: usize,
    ) -> Result<Vec<u64>, Error> {
        const MAX_REQUEST_CHUNK: usize = 1024;

        COUNTERS.call[&(Method::BaseFeeHistory, self.component())].inc();
        let latency = LATENCIES.direct[&Method::BaseFeeHistory].start();
        let mut history = Vec::with_capacity(block_count);
        let from_block = upto_block.saturating_sub(block_count);

        // Here we are requesting `fee_history` from blocks
        // `(from_block; upto_block)` in chunks of size `MAX_REQUEST_CHUNK`
        // starting from the oldest block.
        for chunk_start in (from_block..=upto_block).step_by(MAX_REQUEST_CHUNK) {
            let chunk_end = (chunk_start + MAX_REQUEST_CHUNK).min(upto_block);
            let chunk_size = chunk_end - chunk_start;

            let fee_history = self
                .fee_history(
                    U64::from(chunk_size),
                    web3::BlockNumber::from(chunk_end),
                    None,
                )
                .rpc_context("fee_history")
                .with_arg("chunk_size", &chunk_size)
                .with_arg("block", &chunk_end)
                .await?;
            history.extend(fee_history.base_fee_per_gas);
        }

        latency.observe();
        Ok(history.into_iter().map(|fee| fee.as_u64()).collect())
    }

    async fn get_pending_block_base_fee_per_gas(&self) -> Result<U256, Error> {
        COUNTERS.call[&(Method::PendingBlockBaseFee, self.component())].inc();
        let latency = LATENCIES.direct[&Method::PendingBlockBaseFee].start();

        let block = self
            .get_block_by_number(web3::BlockNumber::Pending, false)
            .rpc_context("get_block_by_number")
            .with_arg("number", &web3::BlockNumber::Pending)
            .with_arg("with_transactions", &false)
            .await?;
        let block = if let Some(block) = block {
            block
        } else {
            // Fallback for local reth. Because of artificial nature of producing blocks in local reth setup
            // there may be no pending block
            self.get_block_by_number(web3::BlockNumber::Latest, false)
                .rpc_context("get_block_by_number")
                .with_arg("number", &web3::BlockNumber::Latest)
                .with_arg("with_transactions", &false)
                .await?
                .expect("Latest block always exists")
        };

        latency.observe();
        // base_fee_per_gas always exists after London fork
        Ok(block.base_fee_per_gas.unwrap())
    }

    async fn get_tx_status(&self, hash: H256) -> Result<Option<ExecutedTxStatus>, Error> {
        COUNTERS.call[&(Method::GetTxStatus, self.component())].inc();
        let latency = LATENCIES.direct[&Method::GetTxStatus].start();

        let receipt = self.tx_receipt(hash).await?;
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
        let transaction = self
            .get_transaction_by_hash(tx_hash)
            .rpc_context("failure_reason#get_transaction_by_hash")
            .with_arg("hash", &tx_hash)
            .await?;
        let receipt = self
            .get_transaction_receipt(tx_hash)
            .rpc_context("failure_reason#get_transaction_receipt")
            .with_arg("hash", &tx_hash)
            .await?;

        match (transaction, receipt) {
            (Some(transaction), Some(receipt)) => {
                let gas_limit = transaction.gas;
                let gas_used = receipt.gas_used;

                let call_request = web3::CallRequest {
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

                let block_number = receipt
                    .block_number
                    .map_or_else(|| web3::BlockNumber::Latest.into(), Into::into);
                let result = self
                    .call(call_request.clone(), block_number)
                    .rpc_context("failure_reason#call")
                    .with_arg("call_request", &call_request)
                    .with_arg("block_number", &block_number)
                    .await;

                let failure_info = match result {
                    Err(err) => {
                        if let ClientError::Call(call_err) = err.as_ref() {
                            let revert_code = call_err.code().into();
                            let message_len =
                                "execution reverted: ".len().min(call_err.message().len());
                            let revert_reason = call_err.message()[message_len..].to_string();

                            Ok(Some(FailureInfo {
                                revert_code,
                                revert_reason,
                                gas_used,
                                gas_limit,
                            }))
                        } else {
                            Err(err.into())
                        }
                    }
                    Ok(_) => Ok(None),
                };

                latency.observe();
                failure_info
            }
            _ => Ok(None),
        }
    }

    async fn get_tx(&self, hash: H256) -> Result<Option<web3::Transaction>, Error> {
        COUNTERS.call[&(Method::GetTx, self.component())].inc();
        let tx = self
            .get_transaction_by_hash(hash)
            .rpc_context("get_transaction_by_hash")
            .with_arg("hash", &hash)
            .await?;
        Ok(tx)
    }

    async fn call_contract_function(
        &self,
        request: web3::CallRequest,
        block: Option<web3::BlockId>,
    ) -> Result<web3::Bytes, Error> {
        let latency = LATENCIES.direct[&Method::CallContractFunction].start();
        let block = block.unwrap_or_else(|| web3::BlockNumber::Latest.into());
        let output_bytes = self
            .call(request.clone(), block)
            .rpc_context("call")
            .with_arg("request", &request)
            .with_arg("block", &block)
            .await?;
        latency.observe();
        Ok(output_bytes)
    }

    async fn tx_receipt(&self, tx_hash: H256) -> Result<Option<web3::TransactionReceipt>, Error> {
        COUNTERS.call[&(Method::TxReceipt, self.component())].inc();
        let latency = LATENCIES.direct[&Method::TxReceipt].start();
        let receipt = self
            .get_transaction_receipt(tx_hash)
            .rpc_context("get_transaction_receipt")
            .with_arg("hash", &tx_hash)
            .await?;
        latency.observe();
        Ok(receipt)
    }

    async fn eth_balance(&self, address: Address) -> Result<U256, Error> {
        COUNTERS.call[&(Method::EthBalance, self.component())].inc();
        let latency = LATENCIES.direct[&Method::EthBalance].start();
        let balance = self
            .get_balance(address, web3::BlockNumber::Latest)
            .rpc_context("get_balance")
            .with_arg("address", &address)
            .await?;
        latency.observe();
        Ok(balance)
    }

    async fn logs(&self, filter: web3::Filter) -> Result<Vec<web3::Log>, Error> {
        COUNTERS.call[&(Method::Logs, self.component())].inc();
        let latency = LATENCIES.direct[&Method::Logs].start();
        let logs = self
            .get_logs(filter.clone())
            .rpc_context("get_logs")
            .with_arg("filter", &filter)
            .await?;
        latency.observe();
        Ok(logs)
    }

    async fn block(&self, block_id: web3::BlockId) -> Result<Option<web3::Block<H256>>, Error> {
        COUNTERS.call[&(Method::Block, self.component())].inc();
        let latency = LATENCIES.direct[&Method::Block].start();
        let block = match block_id {
            web3::BlockId::Hash(hash) => {
                self.get_block_by_hash(hash, false)
                    .rpc_context("get_block_by_hash")
                    .with_arg("hash", &hash)
                    .with_arg("with_transactions", &false)
                    .await?
            }
            web3::BlockId::Number(num) => {
                self.get_block_by_number(num, false)
                    .rpc_context("get_block_by_number")
                    .with_arg("number", &num)
                    .with_arg("with_transactions", &false)
                    .await?
            }
        };
        latency.observe();
        Ok(block)
    }
}

#[cfg(test)]
mod tests {
    use zksync_web3_decl::client::Client;

    use super::*;

    #[test]
    fn client_can_be_cloned() {
        let client = Client::<L1>::http("http://localhost".parse().unwrap())
            .unwrap()
            .build();
        let client: Box<dyn EthInterface> = Box::new(client);
        let _ = client.clone();
    }
}
