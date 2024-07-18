use async_trait::async_trait;
use bitcoin::{Address, Block, OutPoint, Transaction, Txid};
use bitcoincore_rpc::{
    bitcoincore_rpc_json::EstimateMode, json::EstimateSmartFeeResult, Auth, Client, RpcApi,
};

use crate::{traits::BitcoinRpc, types::BitcoinRpcResult};

pub struct BitcoinRpcClient {
    client: Client,
}

#[allow(unused)]
impl BitcoinRpcClient {
    pub fn new(
        url: &str,
        rpc_user: &str,
        rpc_password: &str,
    ) -> Result<Self, bitcoincore_rpc::Error> {
        let auth = Auth::UserPass(rpc_user.to_string(), rpc_password.to_string());
        let client = Client::new(url, auth)?;
        Ok(Self { client })
    }
}

#[allow(unused)]
#[async_trait]
impl BitcoinRpc for BitcoinRpcClient {
    async fn get_balance(&self, address: &Address) -> BitcoinRpcResult<u64> {
        let unspent = self
            .client
            .list_unspent(Some(1), None, Some(&[address]), None, None)?;
        let balance = unspent.iter().map(|u| u.amount.to_sat()).sum();
        Ok(balance)
    }

    async fn send_raw_transaction(&self, tx_hex: &str) -> BitcoinRpcResult<Txid> {
        self.client
            .send_raw_transaction(tx_hex)
            .map_err(|e| e.into())
    }

    async fn list_unspent(&self, address: &Address) -> BitcoinRpcResult<Vec<OutPoint>> {
        let unspent = self
            .client
            .list_unspent(Some(1), None, Some(&[address]), None, None)?;
        Ok(unspent
            .into_iter()
            .map(|u| OutPoint {
                vout: u.vout,
                txid: u.txid,
            })
            .collect())
    }

    async fn get_transaction(&self, txid: &Txid) -> BitcoinRpcResult<Transaction> {
        self.client
            .get_raw_transaction(txid, None)
            .map_err(|e| e.into())
    }

    async fn get_block_count(&self) -> BitcoinRpcResult<u64> {
        self.client.get_block_count().map_err(|e| e.into())
    }

    async fn get_block(&self, block_height: u128) -> BitcoinRpcResult<Block> {
        let block_hash = self.client.get_block_hash(block_height as u64)?;
        self.client.get_block(&block_hash).map_err(|e| e.into())
    }

    async fn get_best_block_hash(&self) -> BitcoinRpcResult<bitcoin::BlockHash> {
        self.client.get_best_block_hash().map_err(|e| e.into())
    }

    async fn get_raw_transaction_info(
        &self,
        txid: &Txid,
        block_hash: Option<&bitcoin::BlockHash>,
    ) -> BitcoinRpcResult<bitcoincore_rpc::json::GetRawTransactionResult> {
        self.client
            .get_raw_transaction_info(txid, block_hash)
            .map_err(|e| e.into())
    }

    async fn estimate_smart_fee(
        &self,
        conf_target: u16,
        estimate_mode: Option<EstimateMode>,
    ) -> BitcoinRpcResult<EstimateSmartFeeResult> {
        self.client
            .estimate_smart_fee(conf_target, estimate_mode)
            .map_err(|e| e.into())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tokio;

    use super::*;
    use crate::{regtest::TestContext, traits::BitcoinRpc};

    #[tokio::test]
    async fn test_get_balance() {
        let context = TestContext::setup(None).await;
        let balance = context
            .client
            .get_balance(&context.test_address)
            .await
            .unwrap();
        assert_eq!(balance, 0, "Initial balance should be 0");
    }

    #[tokio::test]
    async fn test_get_block_count() {
        let context = TestContext::setup(None).await;
        let block_count = context.client.get_block_count().await.unwrap();
        assert!(block_count > 100, "Block count should be greater than 100");
    }

    #[tokio::test]
    async fn test_get_block_size() {
        let context = TestContext::setup(None).await;
        let block = context.client.get_block(1).await.unwrap();
        assert_eq!(block.total_size(), 248usize, "Block version should be 1");
    }

    #[tokio::test]
    async fn test_list_unspent() {
        let context = TestContext::setup(None).await;
        let unspent = context
            .client
            .list_unspent(&context.test_address)
            .await
            .unwrap();
        assert!(
            unspent.is_empty(),
            "Initially, there should be no unspent outputs"
        );
    }

    #[tokio::test]
    async fn test_send_raw_transaction() {
        let context = TestContext::setup(None).await;
        let dummy_tx_hex = "020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff0502cd010101ffffffff0200f2052a01000000160014a8a01aa2b9c7f00bbd59aabfb047e8f3a181d7ed0000000000000000266a24aa21a9ede2f61c3f71d1defd3fa999dfa36953755c690689799962b48bebd836974e8cf90120000000000000000000000000000000000000000000000000000000000000000000000000";
        let result = context.client.send_raw_transaction(dummy_tx_hex).await;
        assert!(result.is_err(), "Sending invalid transaction should fail");
    }

    #[tokio::test]
    async fn test_get_transaction() {
        let context = TestContext::setup(None).await;
        let dummy_txid =
            Txid::from_str("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b")
                .unwrap();
        let result = context.client.get_transaction(&dummy_txid).await;
        assert!(
            result.is_err(),
            "Getting non-existent transaction should fail"
        );
    }
}
