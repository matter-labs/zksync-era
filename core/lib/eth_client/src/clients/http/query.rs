use std::fmt;

use async_trait::async_trait;
use jsonrpsee::core::ClientError;
use zksync_types::{web3, Address, SLChainId, H256, U256, U64};
use zksync_web3_decl::{
    client::{Client, DynClient, EthereumLikeNetwork, ForEthereumLikeNetwork, MockClient, L1, L2},
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    namespaces::EthNamespaceClient,
};

use super::{decl::L1EthNamespaceClient, Method, COUNTERS, LATENCIES};
use crate::{
    types::{ExecutedTxStatus, FailureInfo},
    BaseFees, EthFeeInterface, EthInterface, RawTransactionBytes,
};

const FEE_HISTORY_MAX_REQUEST_CHUNK: usize = 1024;

#[async_trait]
impl<T> EthInterface for T
where
    T: L1EthNamespaceClient + fmt::Debug + Send + Sync,
{
    async fn fetch_chain_id(&self) -> EnrichedClientResult<SLChainId> {
        COUNTERS.call[&(Method::ChainId, self.component())].inc();
        let latency = LATENCIES.direct[&Method::ChainId].start();
        let raw_chain_id = self.chain_id().rpc_context("chain_id").await?;
        latency.observe();
        let chain_id = u64::try_from(raw_chain_id).map_err(|err| {
            let err = ClientError::Custom(format!("invalid chainId: {err}"));
            EnrichedClientError::new(err, "chain_id").with_arg("chain_id", &raw_chain_id)
        })?;
        Ok(SLChainId(chain_id))
    }

    async fn nonce_at_for_account(
        &self,
        account: Address,
        block: web3::BlockNumber,
    ) -> EnrichedClientResult<U256> {
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

    async fn block_number(&self) -> EnrichedClientResult<U64> {
        COUNTERS.call[&(Method::BlockNumber, self.component())].inc();
        let latency = LATENCIES.direct[&Method::BlockNumber].start();
        let block_number = self
            .get_block_number()
            .rpc_context("get_block_number")
            .await?;
        latency.observe();
        Ok(block_number)
    }

    async fn get_gas_price(&self) -> EnrichedClientResult<U256> {
        COUNTERS.call[&(Method::GetGasPrice, self.component())].inc();
        let latency = LATENCIES.direct[&Method::GetGasPrice].start();
        let network_gas_price = self.gas_price().rpc_context("gas_price").await?;
        latency.observe();
        Ok(network_gas_price)
    }

    async fn send_raw_tx(&self, tx: RawTransactionBytes) -> EnrichedClientResult<H256> {
        let latency = LATENCIES.direct[&Method::SendRawTx].start();
        let tx = self
            .send_raw_transaction(web3::Bytes(tx.0))
            .rpc_context("send_raw_transaction")
            .await?;
        latency.observe();
        Ok(tx)
    }

    async fn get_pending_block_base_fee_per_gas(&self) -> EnrichedClientResult<U256> {
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

    async fn get_tx_status(&self, hash: H256) -> EnrichedClientResult<Option<ExecutedTxStatus>> {
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

    async fn failure_reason(&self, tx_hash: H256) -> EnrichedClientResult<Option<FailureInfo>> {
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
                            Err(err)
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

    async fn get_tx(&self, hash: H256) -> EnrichedClientResult<Option<web3::Transaction>> {
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
    ) -> EnrichedClientResult<web3::Bytes> {
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

    async fn tx_receipt(
        &self,
        tx_hash: H256,
    ) -> EnrichedClientResult<Option<web3::TransactionReceipt>> {
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

    async fn eth_balance(&self, address: Address) -> EnrichedClientResult<U256> {
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

    async fn logs(&self, filter: &web3::Filter) -> EnrichedClientResult<Vec<web3::Log>> {
        COUNTERS.call[&(Method::Logs, self.component())].inc();
        let latency = LATENCIES.direct[&Method::Logs].start();
        let logs = self
            .get_logs(filter.clone())
            .rpc_context("get_logs")
            .with_arg("filter", filter)
            .await?;
        latency.observe();
        Ok(logs)
    }

    async fn block(
        &self,
        block_id: web3::BlockId,
    ) -> EnrichedClientResult<Option<web3::Block<H256>>> {
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

// pub trait L1Client: L1EthNamespaceClient + EthInterface + ForEthereumLikeNetwork<Net = L1> {}

// impl L1Client for Box<DynClient<L1>> {}
// impl L1Client for MockClient<L1> {}

macro_rules! impl_l1_eth_fee_interface {
    ($type:ty) => {
        #[async_trait::async_trait]
        impl EthFeeInterface for $type {
            async fn base_fee_history(
                &self,
                upto_block: usize,
                block_count: usize,
            ) -> EnrichedClientResult<Vec<BaseFees>> {
                COUNTERS.call[&(Method::BaseFeeHistory, self.component())].inc();
                let latency = LATENCIES.direct[&Method::BaseFeeHistory].start();
                let mut history = Vec::with_capacity(block_count);
                let from_block = upto_block.saturating_sub(block_count);

                // Here we are requesting `fee_history` from blocks
                // `(from_block; upto_block)` in chunks of size `MAX_REQUEST_CHUNK`
                // starting from the oldest block.
                for chunk_start in (from_block..=upto_block).step_by(FEE_HISTORY_MAX_REQUEST_CHUNK) {
                    let chunk_end = (chunk_start + FEE_HISTORY_MAX_REQUEST_CHUNK).min(upto_block);
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

                    // Check that the lengths are the same.
                    // Per specification, the values should always be provided, and must be 0 for blocks
                    // prior to EIP-4844.
                    // https://ethereum.github.io/execution-apis/api-documentation/
                    if fee_history.base_fee_per_gas.len() != fee_history.base_fee_per_blob_gas.len() {
                        tracing::error!(
                            "base_fee_per_gas and base_fee_per_blob_gas have different lengths: {} and {}",
                            fee_history.base_fee_per_gas.len(),
                            fee_history.base_fee_per_blob_gas.len()
                        );
                    }

                    for (base, blob) in fee_history
                        .base_fee_per_gas
                        .into_iter()
                        .zip(fee_history.base_fee_per_blob_gas)
                    {
                        let fees = BaseFees {
                            base_fee_per_gas: cast_to_u64(base, "base_fee_per_gas")?,
                            base_fee_per_blob_gas: blob,
                            l2_pubdata_price: 0.into()
                        };
                        history.push(fees)
                    }
                }

                latency.observe();
                Ok(history)
            }
        }
    };
}

impl_l1_eth_fee_interface!(Box::<DynClient::<L1>>);
impl_l1_eth_fee_interface!(MockClient::<L1>);

#[async_trait::async_trait]
impl EthFeeInterface for Box<DynClient<L2>> {
    async fn base_fee_history(
        &self,
        upto_block: usize,
        block_count: usize,
    ) -> EnrichedClientResult<Vec<BaseFees>> {
        COUNTERS.call[&(Method::L2FeeHistory, self.component())].inc();
        let latency = LATENCIES.direct[&Method::BaseFeeHistory].start();
        let mut history = Vec::with_capacity(block_count);
        let from_block = upto_block.saturating_sub(block_count);

        // Here we are requesting `fee_history` from blocks
        // `(from_block; upto_block)` in chunks of size `FEE_HISTORY_MAX_REQUEST_CHUNK`
        // starting from the oldest block.
        for chunk_start in (from_block..=upto_block).step_by(FEE_HISTORY_MAX_REQUEST_CHUNK) {
            let chunk_end = (chunk_start + FEE_HISTORY_MAX_REQUEST_CHUNK).min(upto_block);
            let chunk_size = chunk_end - chunk_start;

            let fee_history = EthNamespaceClient::fee_history(
                self,
                U64::from(chunk_size),
                zksync_types::api::BlockNumber::from(chunk_end),
                vec![],
            )
            .rpc_context("fee_history")
            .with_arg("chunk_size", &chunk_size)
            .with_arg("block", &chunk_end)
            .await?;

            // Check that the lengths are the same.
            if fee_history.inner.base_fee_per_gas.len() != fee_history.l2_pubdata_price.len() {
                tracing::error!(
                    "base_fee_per_gas and pubdata_price have different lengths: {} and {}",
                    fee_history.inner.base_fee_per_gas.len(),
                    fee_history.l2_pubdata_price.len()
                );
            }

            for (base, l2_pubdata_price) in fee_history
                .inner
                .base_fee_per_gas
                .into_iter()
                .zip(fee_history.l2_pubdata_price)
            {
                let fees = BaseFees {
                    base_fee_per_gas: cast_to_u64(base, "base_fee_per_gas")?,
                    base_fee_per_blob_gas: 0.into(),
                    l2_pubdata_price,
                };
                history.push(fees)
            }
        }

        latency.observe();
        Ok(history)
    }
}

/// Non-panicking conversion to u64.
fn cast_to_u64(value: U256, tag: &str) -> EnrichedClientResult<u64> {
    u64::try_from(value).map_err(|_| {
        let err = ClientError::Custom(format!("{tag} value does not fit in u64"));
        EnrichedClientError::new(err, "cast_to_u64").with_arg("value", &value)
    })
}
#[cfg(test)]
mod tests {
    use zksync_web3_decl::client::{Client, L1};

    /// This test makes sure that we can instantiate a client with an HTTPS provider.
    /// The need for this test was caused by feature collisions for `rustls` in our dependency graph,
    /// which caused this test to panic.
    #[tokio::test]
    async fn test_https_provider() {
        let url = "https://rpc.flashbots.net/";
        let _client = Client::<L1>::http(url.parse().unwrap()).unwrap().build();
        // No need to do anything; if the client was created and we didn't panic, we're good.
    }
}
