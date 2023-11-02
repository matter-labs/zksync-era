#![allow(clippy::derive_partial_eq_without_eq, clippy::format_push_string)]

use std::env;

// Built-in deps
pub use sqlx::Error as SqlxError;
use sqlx::{postgres::Postgres, Connection, PgConnection, Transaction};
// External imports
use anyhow::Context as _;
use sqlx::pool::PoolConnection;
pub use sqlx::types::BigDecimal;

use zksync_db_utils::connection_holder::ConnectionHolder;

// Local imports
use crate::accounts_dal::AccountsDal;
use crate::basic_witness_input_producer_dal::BasicWitnessInputProducerDal;
use crate::blocks_dal::BlocksDal;
use crate::blocks_web3_dal::BlocksWeb3Dal;
pub use crate::connection::ServerConnectionPool;
use crate::contract_verification_dal::ContractVerificationDal;
use crate::eth_sender_dal::EthSenderDal;
use crate::events_dal::EventsDal;
use crate::events_web3_dal::EventsWeb3Dal;
use crate::proof_generation_dal::ProofGenerationDal;
use crate::protocol_versions_dal::ProtocolVersionsDal;
use crate::protocol_versions_web3_dal::ProtocolVersionsWeb3Dal;
use crate::storage_dal::StorageDal;
use crate::storage_logs_dal::StorageLogsDal;
use crate::storage_logs_dedup_dal::StorageLogsDedupDal;
use crate::storage_web3_dal::StorageWeb3Dal;
use crate::sync_dal::SyncDal;
use crate::system_dal::SystemDal;
use crate::tokens_dal::TokensDal;
use crate::tokens_web3_dal::TokensWeb3Dal;
use crate::transactions_dal::TransactionsDal;
use crate::transactions_web3_dal::TransactionsWeb3Dal;

pub mod accounts_dal;
pub mod basic_witness_input_producer_dal;
pub mod blocks_dal;
pub mod blocks_web3_dal;
pub mod connection;
pub mod contract_verification_dal;
pub mod eth_sender_dal;
pub mod events_dal;
pub mod events_web3_dal;
pub mod healthcheck;
mod models;
pub mod proof_generation_dal;
pub mod protocol_versions_dal;
pub mod protocol_versions_web3_dal;
pub mod storage_dal;
pub mod storage_logs_dal;
pub mod storage_logs_dedup_dal;
pub mod storage_web3_dal;
pub mod sync_dal;
pub mod system_dal;
pub mod tokens_dal;
pub mod tokens_web3_dal;
pub mod transactions_dal;
pub mod transactions_web3_dal;

#[cfg(test)]
mod tests;

/// Obtains the master database URL from the environment variable.
pub fn get_master_database_url() -> anyhow::Result<String> {
    env::var("DATABASE_URL").context("DATABASE_URL must be set")
}

/// Obtains the replica database URL from the environment variable.
pub fn get_replica_database_url() -> anyhow::Result<String> {
    match env::var("DATABASE_REPLICA_URL") {
        Ok(url) => Ok(url),
        Err(_) => get_master_database_url(),
    }
}

/// Obtains the test database URL from the environment variable.
pub fn get_test_database_url() -> anyhow::Result<String> {
    env::var("TEST_DATABASE_URL").context("TEST_DATABASE_URL must be set")
}

/// Storage processor is the main storage interaction point.
/// It holds down the connection (either direct or pooled) to the database
/// and provide methods to obtain different storage schemas.
#[derive(Debug)]
pub struct ServerStorageProcessor<'a> {
    conn: ConnectionHolder<'a>,
    in_transaction: bool,
}

impl<'a> ServerStorageProcessor<'a> {
    pub async fn establish_connection(
        connect_to_master: bool,
    ) -> anyhow::Result<ServerStorageProcessor<'static>> {
        let database_url = if connect_to_master {
            get_master_database_url()?
        } else {
            get_replica_database_url()?
        };
        let connection = PgConnection::connect(&database_url)
            .await
            .context("PgConnectio::connect()")?;
        Ok(ServerStorageProcessor {
            conn: ConnectionHolder::Direct(connection),
            in_transaction: false,
        })
    }

    pub async fn start_transaction<'c: 'b, 'b>(
        &'c mut self,
    ) -> sqlx::Result<ServerStorageProcessor<'b>> {
        let transaction = self.conn().begin().await?;
        let mut processor = ServerStorageProcessor::from_transaction(transaction);
        processor.in_transaction = true;
        Ok(processor)
    }

    /// Checks if the `ServerStorageProcessor` is currently within database transaction.
    pub fn in_transaction(&self) -> bool {
        self.in_transaction
    }

    pub fn from_transaction(conn: Transaction<'a, Postgres>) -> Self {
        Self {
            conn: ConnectionHolder::Transaction(conn),
            in_transaction: true,
        }
    }

    pub async fn commit(self) -> sqlx::Result<()> {
        if let ConnectionHolder::Transaction(transaction) = self.conn {
            transaction.commit().await
        } else {
            panic!("ServerStorageProcessor::commit can only be invoked after calling ServerStorageProcessor::begin_transaction");
        }
    }

    /// Creates a `ServerStorageProcessor` using a pool of connections.
    /// This method borrows one of the connections from the pool, and releases it
    /// after `drop`.
    pub fn from_pool(conn: PoolConnection<Postgres>) -> Self {
        Self {
            conn: ConnectionHolder::Pooled(conn),
            in_transaction: false,
        }
    }

    fn conn(&mut self) -> &mut PgConnection {
        match &mut self.conn {
            ConnectionHolder::Pooled(conn) => conn,
            ConnectionHolder::Direct(conn) => conn,
            ConnectionHolder::Transaction(conn) => conn,
        }
    }

    pub fn transactions_dal(&mut self) -> TransactionsDal<'_, 'a> {
        TransactionsDal { storage: self }
    }

    pub fn transactions_web3_dal(&mut self) -> TransactionsWeb3Dal<'_, 'a> {
        TransactionsWeb3Dal { storage: self }
    }

    pub fn accounts_dal(&mut self) -> AccountsDal<'_, 'a> {
        AccountsDal { storage: self }
    }

    pub fn blocks_dal(&mut self) -> BlocksDal<'_, 'a> {
        BlocksDal { storage: self }
    }

    pub fn blocks_web3_dal(&mut self) -> BlocksWeb3Dal<'_, 'a> {
        BlocksWeb3Dal { storage: self }
    }

    pub fn eth_sender_dal(&mut self) -> EthSenderDal<'_, 'a> {
        EthSenderDal { storage: self }
    }

    pub fn events_dal(&mut self) -> EventsDal<'_, 'a> {
        EventsDal { storage: self }
    }

    pub fn events_web3_dal(&mut self) -> EventsWeb3Dal<'_, 'a> {
        EventsWeb3Dal { storage: self }
    }

    pub fn storage_dal(&mut self) -> StorageDal<'_, 'a> {
        StorageDal { storage: self }
    }

    pub fn storage_web3_dal(&mut self) -> StorageWeb3Dal<'_, 'a> {
        StorageWeb3Dal { storage: self }
    }

    pub fn storage_logs_dal(&mut self) -> StorageLogsDal<'_, 'a> {
        StorageLogsDal { storage: self }
    }

    pub fn storage_logs_dedup_dal(&mut self) -> StorageLogsDedupDal<'_, 'a> {
        StorageLogsDedupDal { storage: self }
    }

    pub fn tokens_dal(&mut self) -> TokensDal<'_, 'a> {
        TokensDal { storage: self }
    }

    pub fn tokens_web3_dal(&mut self) -> TokensWeb3Dal<'_, 'a> {
        TokensWeb3Dal { storage: self }
    }

    pub fn contract_verification_dal(&mut self) -> ContractVerificationDal<'_, 'a> {
        ContractVerificationDal { storage: self }
    }

    pub fn protocol_versions_dal(&mut self) -> ProtocolVersionsDal<'_, 'a> {
        ProtocolVersionsDal { storage: self }
    }

    pub fn protocol_versions_web3_dal(&mut self) -> ProtocolVersionsWeb3Dal<'_, 'a> {
        ProtocolVersionsWeb3Dal { storage: self }
    }

    pub fn sync_dal(&mut self) -> SyncDal<'_, 'a> {
        SyncDal { storage: self }
    }

    pub fn system_dal(&mut self) -> SystemDal<'_, 'a> {
        SystemDal { storage: self }
    }

    pub fn basic_witness_input_producer_dal(&mut self) -> BasicWitnessInputProducerDal<'_, 'a> {
        BasicWitnessInputProducerDal { storage: self }
    }

    pub fn proof_generation_dal(&mut self) -> ProofGenerationDal<'_, 'a> {
        ProofGenerationDal { storage: self }
    }
}
