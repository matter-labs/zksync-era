use async_trait::async_trait;
use bitcoin::{Address, Network, TxOut, Txid};
use bitcoincore_rpc::json::EstimateMode;

use crate::{
    traits::{BitcoinOps, BitcoinRpc},
    types::BitcoinClientResult,
};

mod rpc_client;
#[allow(unused)]
pub use rpc_client::BitcoinRpcClient;

use crate::types::BitcoinError;

#[allow(unused)]
pub struct BitcoinClient {
    rpc: Box<dyn BitcoinRpc>,
    network: Network,
}

#[async_trait]
impl BitcoinOps for BitcoinClient {
    async fn new(rpc_url: &str, network: &str) -> BitcoinClientResult<Self>
    where
        Self: Sized,
    {
        // TODO: change rpc_user & rpc_password here, move it to args
        let rpc = Box::new(BitcoinRpcClient::new(rpc_url, "rpcuser", "rpcpassword")?);
        let network = match network.to_lowercase().as_str() {
            "mainnet" => Network::Bitcoin,
            "testnet" => Network::Testnet,
            "regtest" => Network::Regtest,
            _ => return Err(BitcoinError::InvalidNetwork(network.to_string())),
        };

        Ok(Self { rpc, network })
    }

    async fn get_balance(&self, address: &Address) -> BitcoinClientResult<u128> {
        let balance = self.rpc.get_balance(address).await?;
        Ok(balance as u128)
    }

    async fn broadcast_signed_transaction(
        &self,
        signed_transaction: &str,
    ) -> BitcoinClientResult<Txid> {
        let txid = self.rpc.send_raw_transaction(signed_transaction).await?;
        Ok(txid)
    }

    async fn fetch_utxos(&self, address: &Address) -> BitcoinClientResult<Vec<TxOut>> {
        let outpoints = self.rpc.list_unspent(address).await?;

        let mut txouts = Vec::new();
        for outpoint in outpoints {
            let tx = self.rpc.get_transaction(&outpoint.txid).await?;
            let txout = tx
                .output
                .get(outpoint.vout as usize)
                .ok_or(BitcoinError::InvalidOutpoint(outpoint.to_string()))?;
            txouts.push(txout.clone());
        }

        Ok(txouts)
    }

    async fn check_tx_confirmation(&self, txid: &Txid) -> BitcoinClientResult<bool> {
        let latest_block_hash = self.rpc.get_best_block_hash().await?;
        let tx_info = self
            .rpc
            .get_raw_transaction_info(txid, Some(&latest_block_hash))
            .await?;

        match tx_info.confirmations {
            Some(confirmations) => Ok(confirmations > 0),
            None => Ok(false),
        }
    }

    async fn fetch_block_height(&self) -> BitcoinClientResult<u128> {
        let height = self.rpc.get_block_count().await?;
        Ok(height as u128)
    }

    async fn fetch_and_parse_block(&self, block_height: u128) -> BitcoinClientResult<String> {
        let _block = self.rpc.get_block(block_height).await?;
        todo!()
    }

    async fn estimate_fee(&self, conf_target: u16) -> BitcoinClientResult<u64> {
        let estimation = self
            .rpc
            .estimate_smart_fee(conf_target, Some(EstimateMode::Economical))
            .await?;

        match estimation.fee_rate {
            Some(fee_rate) => Ok(fee_rate.to_sat()),
            None => {
                let err = match estimation.errors {
                    Some(errors) => errors.join(", "),
                    None => "Unknown error during fee estimation".to_string(),
                };
                Err(BitcoinError::FeeEstimationFailed(err))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bitcoin::Network;

    use super::*;
    use crate::{regtest::TestContext, traits::BitcoinOps};

    #[tokio::test]
    async fn test_new() {
        let context = TestContext::setup().await;
        let client = BitcoinClient::new(&context._regtest.get_url(), "regtest")
            .await
            .expect("Failed to create BitcoinClient");

        assert_eq!(client.network, Network::Regtest);
    }

    #[ignore]
    #[tokio::test]
    async fn test_get_balance() {
        let context = TestContext::setup().await;
        let _client = BitcoinClient::new(&context._regtest.get_url(), "regtest")
            .await
            .expect("Failed to create BitcoinClient");

        // random test address fails
        // let balance = client
        //     .get_balance(&context.test_address)
        //     .await
        //     .expect("Failed to get balance");
        //
        // assert!(balance > 0, "Balance should be greater than 0");
    }

    #[ignore]
    #[tokio::test]
    async fn test_fetch_utxos() {
        let context = TestContext::setup().await;
        let _client = BitcoinClient::new(&context._regtest.get_url(), "regtest")
            .await
            .expect("Failed to create BitcoinClient");

        // random test address fails
        // let utxos = client
        //     .fetch_utxos(&context.test_address)
        //     .await
        //     .expect("Failed to fetch UTXOs");

        // assert!(!utxos.is_empty(), "UTXOs should not be empty");
    }

    #[tokio::test]
    async fn test_fetch_block_height() {
        let context = TestContext::setup().await;
        let client = BitcoinClient::new(&context._regtest.get_url(), "regtest")
            .await
            .expect("Failed to create BitcoinClient");

        let block_height = client
            .fetch_block_height()
            .await
            .expect("Failed to fetch block height");

        assert!(block_height > 0, "Block height should be greater than 0");
    }

    #[ignore]
    #[tokio::test]
    async fn test_estimate_fee() {
        let context = TestContext::setup().await;
        let client = BitcoinClient::new(&context._regtest.get_url(), "regtest")
            .await
            .expect("Failed to create BitcoinClient");

        // error: Insufficient data or no feerate found
        let fee = client
            .estimate_fee(6)
            .await
            .expect("Failed to estimate fee");

        assert!(fee > 0, "Estimated fee should be greater than 0");
    }
}
