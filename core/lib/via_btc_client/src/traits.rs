use async_trait::async_trait;
use bitcoin::{Address, Block, OutPoint, Transaction, Txid, TxOut};

use crate::types;

#[allow(dead_code)]
#[async_trait]
pub trait BitcoinOps: Send + Sync {
    async fn new(
        rpc_url: &str,
        network: &str,
    ) -> types::BitcoinClientResult<Self>
    where
        Self: Sized;
    async fn get_balance(&self, address: &Address) -> types::BitcoinClientResult<u128>;
    async fn broadcast_signed_transaction(
        &self,
        // TODO: change type here
        signed_transaction: &str,
    ) -> types::BitcoinClientResult<Txid>;
    async fn fetch_utxos(&self, address: &Address) -> types::BitcoinClientResult<Vec<TxOut>>;
    async fn check_tx_confirmation(&self, txid: &Txid) -> types::BitcoinClientResult<bool>;
    async fn fetch_block_height(&self) -> types::BitcoinClientResult<u128>;
    async fn fetch_and_parse_block(&self, block_height: u128)
        -> types::BitcoinClientResult<String>;
    async fn estimate_fee(&self, conf_target: u16) -> types::BitcoinClientResult<u64>;
}

#[allow(dead_code)]
#[async_trait]
pub trait BitcoinRpc: Send + Sync {
    async fn get_balance(&self, address: &Address) -> types::BitcoinRpcResult<u64>;
    async fn send_raw_transaction(&self, tx_hex: &str) -> types::BitcoinRpcResult<Txid>;
    async fn list_unspent(&self, address: &Address) -> types::BitcoinRpcResult<Vec<OutPoint>>;
    async fn get_transaction(&self, tx_id: &Txid) -> types::BitcoinRpcResult<Transaction>;
    async fn get_block_count(&self) -> types::BitcoinRpcResult<u64>;
    async fn get_block(&self, block_height: u128) -> types::BitcoinRpcResult<Block>;
    async fn get_best_block_hash(&self) -> types::BitcoinRpcResult<bitcoin::BlockHash>;
    async fn get_raw_transaction_info(
        &self,
        txid: &Txid,
        block_hash: Option<&bitcoin::BlockHash>,
    ) -> types::BitcoinRpcResult<bitcoincore_rpc::json::GetRawTransactionResult>;
    async fn estimate_smart_fee(
        &self,
        conf_target: u16,
        estimate_mode: Option<bitcoincore_rpc::json::EstimateMode>,
    ) -> types::BitcoinRpcResult<bitcoincore_rpc::json::EstimateSmartFeeResult>;
}

#[allow(dead_code)]
#[async_trait]
pub trait BitcoinSigner: Send + Sync {
    async fn new(private_key: &str) -> types::BitcoinSignerResult<Self>
    where
        Self: Sized;
    async fn sign_transaction(
        &self,
        unsigned_transaction: &str,
    ) -> types::BitcoinSignerResult<String>;
}

#[allow(dead_code)]
#[async_trait]
pub trait BitcoinInscriber: Send + Sync {
    async fn new(config: &str) -> types::BitcoinInscriberResult<Self>
    where
        Self: Sized;
    async fn inscribe(
        &self,
        message_type: &str,
        data: &str,
    ) -> types::BitcoinInscriberResult<String>;
}

#[allow(dead_code)]
#[async_trait]
pub trait BitcoinInscriptionIndexer: Send + Sync {
    async fn new(config: &str) -> types::BitcoinInscriptionIndexerResult<Self>
    where
        Self: Sized;
    async fn get_inscription_messages(
        &self,
        starting_block: u128,
        ending_block: u128,
    ) -> types::BitcoinInscriptionIndexerResult<Vec<&str>>;
    async fn get_specific_block_inscription_messages(
        &self,
        block_height: u128,
    ) -> types::BitcoinInscriptionIndexerResult<Vec<&str>>;
}

#[allow(dead_code)]
#[async_trait]
pub trait BitcoinWithdrawalTransactionBuilder: Send + Sync {
    async fn new(config: &str) -> types::BitcoinTransactionBuilderResult<Self>
    where
        Self: Sized;
    async fn build_withdrawal_transaction(
        &self,
        address: &str,
        amount: u128,
    ) -> types::BitcoinTransactionBuilderResult<String>;
}
