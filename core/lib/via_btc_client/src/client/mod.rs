use async_trait::async_trait;
use bitcoin::{address::NetworkUnchecked, Address, Network, Txid};

use crate::{
    traits::{BitcoinOps, BitcoinRpc},
    types::BitcoinClientResult,
};

mod rpc_client;
#[allow(unused)]
pub use rpc_client::BitcoinRpcClient;

#[allow(unused)]
pub struct BitcoinClient {
    rpc: Box<dyn BitcoinRpc>,
    network: Network,
    sender_address: Address,
}

impl BitcoinClient {
    pub fn new(rpc: Box<dyn BitcoinRpc>, network: Network, sender_address: Address) -> Self {
        Self {
            rpc,
            network,
            sender_address,
        }
    }
}

#[async_trait]
impl BitcoinOps for BitcoinClient {
    async fn new(rpc_url: &str, network: Network, sender_address: &str) -> BitcoinClientResult<Self>
    where
        Self: Sized,
    {
        // TODO: change rpc_user & rpc_password here, move it to args
        let rpc = Box::new(BitcoinRpcClient::new(rpc_url, "rpcuser", "rpcpassword")?);
        let sender_address = sender_address
            .parse::<Address<NetworkUnchecked>>()?
            .require_network(network)?;

        Ok(Self::new(rpc, network, sender_address))
    }

    async fn get_balance(&self, address: &str) -> BitcoinClientResult<u128> {
        let address = address.parse::<Address<NetworkUnchecked>>()?;
        // TODO: change assume_checked here
        let balance = self.rpc.get_balance(&address.assume_checked()).await?;
        Ok(balance as u128)
    }

    async fn broadcast_signed_transaction(
        &self,
        signed_transaction: &str,
    ) -> BitcoinClientResult<String> {
        let txid = self.rpc.send_raw_transaction(signed_transaction).await?;
        Ok(txid.to_string())
    }

    async fn fetch_utxos(&self, address: &str) -> BitcoinClientResult<Vec<String>> {
        let address = address.parse::<Address<NetworkUnchecked>>()?;
        // TODO: change assume_checked here
        let utxos = self.rpc.list_unspent(&address.assume_checked()).await?;
        Ok(utxos.iter().map(|utxo| utxo.clone().to_string()).collect())
    }

    async fn check_tx_confirmation(&self, txid: &str) -> BitcoinClientResult<bool> {
        let txid = Txid::from_raw_hash(txid.parse()?);
        let latest_block_hash = self.rpc.get_best_block_hash().await?;
        let tx_info = self
            .rpc
            .get_raw_transaction_info(&txid, Some(&latest_block_hash))
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

    async fn fetch_and_parse_block(&self, block_height: u128) -> BitcoinClientResult<&str> {
        let _block = self.rpc.get_block(block_height).await?;
        todo!()
    }
}
