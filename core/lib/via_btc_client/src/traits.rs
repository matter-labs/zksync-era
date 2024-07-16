use async_trait::async_trait;

use crate::types;

#[allow(dead_code)]
#[async_trait]
pub trait BitcoinClient: Send + Sync {
    async fn new(rpc_url: &str) -> types::BitcoinClientResult<Self>
    where
        Self: Sized;
    async fn get_balance(&self, address: &str) -> types::BitcoinClientResult<u128>;
    async fn broadcast_signed_transaction(
        &self,
        signed_transaction: &str,
    ) -> types::BitcoinClientResult<String>;
    async fn fetch_utxos(&self, address: &str) -> types::BitcoinClientResult<Vec<&str>>;
    async fn check_tx_confirmation(&self, txid: &str) -> types::BitcoinClientResult<bool>;
    async fn fetch_block_height(&self) -> types::BitcoinClientResult<u128>;
    async fn fetch_and_parse_block(&self, block_height: u128) -> types::BitcoinClientResult<&str>;
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
