use std::fmt;

use async_trait::async_trait;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::{
    aggregated_operations::AggregatedActionType, eth_sender::EthTxBlobSidecar, Address,
};

use crate::EthSenderError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransactionStatus {
    Pending,
    Confirmed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct GetTxResponse {
    pub idempotency_key: String,
    pub tx_hash: Option<Vec<u8>>,
    pub status: TransactionStatus,
}

#[async_trait]
pub(super) trait EthSenderInterface: 'static + Sync + Send + fmt::Debug {
    async fn get_tx(&mut self, idempotency_key: String) -> anyhow::Result<GetTxResponse>;

    async fn send_tx(
        &mut self,
        idempotency_key: String,
        raw_tx: Vec<u8>,
        tx_type: AggregatedActionType,
        contract_address: Address,
        blob_sidecar: Option<EthTxBlobSidecar>,
    ) -> anyhow::Result<()>;
}

#[derive(Debug)]
struct DatabaseEthSenderInterface {
    pool: ConnectionPool<Core>,
    base_nonce: u64,
    base_nonce_custom_commit_sender: Option<u64>,
    custom_commit_sender_addr: Option<Address>,
}

impl DatabaseEthSenderInterface {
    async fn get_next_nonce(
        &self,
        storage: &mut Connection<'_, Core>,
        from_addr: Option<Address>,
    ) -> Result<u64, EthSenderError> {
        let db_nonce = storage
            .eth_sender_dal()
            .get_next_nonce(from_addr)
            .await
            .unwrap()
            .unwrap_or(0);
        // Between server starts we can execute some txs using operator account or remove some txs from the database
        // At the start we have to consider this fact and get the max nonce.
        Ok(if from_addr.is_none() {
            db_nonce.max(self.base_nonce)
        } else {
            db_nonce.max(
                self.base_nonce_custom_commit_sender
                    .expect("custom base nonce is expected to be initialized; qed"),
            )
        })
    }
}

impl EthSenderInterface for DatabaseEthSenderInterface {
    async fn get_tx(&mut self, id: String) -> anyhow::Result<Option<GetTxResponse>> {
        let mut storage = self.pool.connection_tagged("eth_sender_interface").await?;
        let status = storage
            .eth_sender_dal()
            .get_eth_tx_by_idempotency_key(id)
            .await?;
        if let Some(status) = status {
            Some(GetTxResponse {
                id: status.id,
                tx_hash: status.tx_hash,
                status: match (status.is_failed, status.is_confirmed) {
                    (_, true) => TransactionStatus::Confirmed,
                    (true, _) => TransactionStatus::Failed,
                    (_, _) => TransactionStatus::Pending,
                },
            })
        } else {
            Ok(None)
        }
    }

    async fn send_tx(
        &mut self,
        id: String,
        raw_tx: Vec<u8>,
        tx_type: AggregatedActionType,
        contract_address: Address,
        blob_sidecar: Option<EthTxBlobSidecar>,
    ) -> anyhow::Result<()> {
        let mut storage = self.pool.connection_tagged("eth_sender_interface").await?;
        let sender_addr = match tx_type {
            AggregatedActionType::Commit => self.custom_commit_sender_addr,
            _ => None,
        };
        let nonce = self.get_next_nonce(&mut storage, sender_addr).await?;
        storage
            .eth_sender_dal()
            .save_eth_tx(
                Some(id),
                nonce,
                raw_tx,
                tx_type,
                contract_address,
                0,
                sender_addr,
                blob_sidecar,
            )
            .await?;
        Ok(())
    }
}
