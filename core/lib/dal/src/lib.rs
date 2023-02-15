#![allow(clippy::derive_partial_eq_without_eq, clippy::format_push_string)]

use std::env;

// Built-in deps
use async_std::task::block_on;
pub use sqlx::Error as SqlxError;
use sqlx::{postgres::Postgres, Connection, PgConnection, Transaction};
// External imports
use sqlx::pool::PoolConnection;
pub use sqlx::types::BigDecimal;

// Local imports
use crate::blocks_dal::BlocksDal;
use crate::blocks_web3_dal::BlocksWeb3Dal;
use crate::connection::holder::ConnectionHolder;
pub use crate::connection::ConnectionPool;
use crate::eth_sender_dal::EthSenderDal;
use crate::events_dal::EventsDal;
use crate::events_web3_dal::EventsWeb3Dal;
use crate::explorer::ExplorerIntermediator;
use crate::fee_monitor_dal::FeeMonitorDal;
use crate::gpu_prover_queue_dal::GpuProverQueueDal;
use crate::prover_dal::ProverDal;
use crate::storage_dal::StorageDal;
use crate::storage_load_dal::StorageLoadDal;
use crate::storage_logs_dal::StorageLogsDal;
use crate::storage_logs_dedup_dal::StorageLogsDedupDal;
use crate::storage_web3_dal::StorageWeb3Dal;
use crate::tokens_dal::TokensDal;
use crate::tokens_web3_dal::TokensWeb3Dal;
use crate::transactions_dal::TransactionsDal;
use crate::transactions_web3_dal::TransactionsWeb3Dal;
use crate::witness_generator_dal::WitnessGeneratorDal;

pub mod blocks_dal;
pub mod blocks_web3_dal;
pub mod connection;
pub mod eth_sender_dal;
pub mod events_dal;
pub mod events_web3_dal;
pub mod explorer;
pub mod fee_monitor_dal;
pub mod gpu_prover_queue_dal;
mod models;
pub mod prover_dal;
pub mod storage_dal;
pub mod storage_load_dal;
pub mod storage_logs_dal;
pub mod storage_logs_dedup_dal;
pub mod storage_web3_dal;
pub mod time_utils;
pub mod tokens_dal;
pub mod tokens_web3_dal;
pub mod transactions_dal;
pub mod transactions_web3_dal;
pub mod witness_generator_dal;

#[cfg(test)]
mod tests;

/// Obtains the master database URL from the environment variable.
pub fn get_master_database_url() -> String {
    env::var("DATABASE_URL").expect("DATABASE_URL must be set")
}

/// Obtains the replica database URL from the environment variable.
pub fn get_replica_database_url() -> String {
    env::var("DATABASE_REPLICA_URL").unwrap_or_else(|_| get_master_database_url())
}

/// Obtains the test database URL from the environment variable.
pub fn get_test_database_url() -> String {
    env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set")
}

/// Storage processor is the main storage interaction point.
/// It holds down the connection (either direct or pooled) to the database
/// and provide methods to obtain different storage schemas.
#[derive(Debug)]
pub struct StorageProcessor<'a> {
    conn: ConnectionHolder<'a>,
    in_transaction: bool,
}

impl<'a> StorageProcessor<'a> {
    /// Creates a `StorageProcessor` using an unique sole connection to the database.
    pub async fn establish_connection(connect_to_master: bool) -> StorageProcessor<'static> {
        let database_url = if connect_to_master {
            get_master_database_url()
        } else {
            get_replica_database_url()
        };
        let connection = PgConnection::connect(&database_url).await.unwrap();
        StorageProcessor {
            conn: ConnectionHolder::Direct(connection),
            in_transaction: false,
        }
    }

    pub async fn start_transaction<'c: 'b, 'b>(&'c mut self) -> StorageProcessor<'b> {
        let transaction = self.conn().begin().await.unwrap();

        let mut processor = StorageProcessor::from_transaction(transaction);
        processor.in_transaction = true;

        processor
    }

    pub fn start_transaction_blocking<'c: 'b, 'b>(&'c mut self) -> StorageProcessor<'b> {
        block_on(self.start_transaction())
    }

    /// Checks if the `StorageProcessor` is currently within database transaction.
    pub fn in_transaction(&self) -> bool {
        self.in_transaction
    }

    pub fn from_transaction(conn: Transaction<'_, Postgres>) -> StorageProcessor<'_> {
        StorageProcessor {
            conn: ConnectionHolder::Transaction(conn),
            in_transaction: true,
        }
    }

    pub fn from_test_transaction<'b>(
        conn: &'b mut Transaction<'static, Postgres>,
    ) -> StorageProcessor<'b> {
        StorageProcessor {
            conn: ConnectionHolder::TestTransaction(conn),
            in_transaction: true,
        }
    }

    pub async fn commit(self) {
        if let ConnectionHolder::Transaction(transaction) = self.conn {
            transaction.commit().await.unwrap();
        } else {
            panic!("StorageProcessor::commit can only be invoked after calling StorageProcessor::begin_transaction");
        }
    }

    pub fn commit_blocking(self) {
        block_on(self.commit())
    }

    /// Creates a `StorageProcessor` using a pool of connections.
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
            ConnectionHolder::TestTransaction(conn) => conn,
        }
    }

    pub fn transactions_dal(&mut self) -> TransactionsDal<'_, 'a> {
        TransactionsDal { storage: self }
    }

    pub fn transactions_web3_dal(&mut self) -> TransactionsWeb3Dal<'_, 'a> {
        TransactionsWeb3Dal { storage: self }
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

    pub fn storage_load_dal(&mut self) -> StorageLoadDal<'_, 'a> {
        StorageLoadDal { storage: self }
    }

    pub fn tokens_dal(&mut self) -> TokensDal<'_, 'a> {
        TokensDal { storage: self }
    }

    pub fn tokens_web3_dal(&mut self) -> TokensWeb3Dal<'_, 'a> {
        TokensWeb3Dal { storage: self }
    }

    pub fn prover_dal(&mut self) -> ProverDal<'_, 'a> {
        ProverDal { storage: self }
    }

    pub fn witness_generator_dal(&mut self) -> WitnessGeneratorDal<'_, 'a> {
        WitnessGeneratorDal { storage: self }
    }

    pub fn explorer(&mut self) -> ExplorerIntermediator<'_, 'a> {
        ExplorerIntermediator { storage: self }
    }

    pub fn fee_monitor_dal(&mut self) -> FeeMonitorDal<'_, 'a> {
        FeeMonitorDal { storage: self }
    }

    pub fn gpu_prover_queue_dal(&mut self) -> GpuProverQueueDal<'_, 'a> {
        GpuProverQueueDal { storage: self }
    }
}
