#![allow(clippy::derive_partial_eq_without_eq, clippy::format_push_string)]

use std::env;

// Built-in deps
pub use sqlx::Error as SqlxError;
use sqlx::{Connection as _, PgConnection};
// External imports
use anyhow::Context as _;
pub use sqlx::types::BigDecimal;

// Local imports
use crate::accounts_dal::AccountsDal;
use crate::blocks_dal::BlocksDal;
use crate::blocks_web3_dal::BlocksWeb3Dal;
use crate::connection::holder::ConnectionRef;
pub use crate::connection::holder::{Acquire, Connection, Transaction};
pub use crate::connection::ConnectionPool;
use crate::contract_verification_dal::ContractVerificationDal;
use crate::eth_sender_dal::EthSenderDal;
use crate::events_dal::EventsDal;
use crate::events_web3_dal::EventsWeb3Dal;
use crate::fri_gpu_prover_queue_dal::FriGpuProverQueueDal;
use crate::fri_proof_compressor_dal::FriProofCompressorDal;
use crate::fri_protocol_versions_dal::FriProtocolVersionsDal;
use crate::fri_prover_dal::FriProverDal;
use crate::fri_scheduler_dependency_tracker_dal::FriSchedulerDependencyTrackerDal;
use crate::fri_witness_generator_dal::FriWitnessGeneratorDal;
use crate::gpu_prover_queue_dal::GpuProverQueueDal;
use crate::proof_generation_dal::ProofGenerationDal;
use crate::protocol_versions_dal::ProtocolVersionsDal;
use crate::protocol_versions_web3_dal::ProtocolVersionsWeb3Dal;
use crate::prover_dal::ProverDal;
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
use crate::witness_generator_dal::WitnessGeneratorDal;

#[macro_use]
mod macro_utils;
pub mod accounts_dal;
pub mod blocks_dal;
pub mod blocks_web3_dal;
pub mod connection;
pub mod contract_verification_dal;
pub mod eth_sender_dal;
pub mod events_dal;
pub mod events_web3_dal;
pub mod fri_gpu_prover_queue_dal;
pub mod fri_proof_compressor_dal;
pub mod fri_protocol_versions_dal;
pub mod fri_prover_dal;
pub mod fri_scheduler_dependency_tracker_dal;
pub mod fri_witness_generator_dal;
pub mod gpu_prover_queue_dal;
pub mod healthcheck;
mod instrument;
mod metrics;
mod models;
pub mod proof_generation_dal;
pub mod protocol_versions_dal;
pub mod protocol_versions_web3_dal;
pub mod prover_dal;
pub mod storage_dal;
pub mod storage_logs_dal;
pub mod storage_logs_dedup_dal;
pub mod storage_web3_dal;
pub mod sync_dal;
pub mod system_dal;
pub mod time_utils;
pub mod tokens_dal;
pub mod tokens_web3_dal;
pub mod transactions_dal;
pub mod transactions_web3_dal;
pub mod witness_generator_dal;

#[cfg(test)]
mod tests;

/// Obtains the master database URL from the environment variable.
pub fn get_master_database_url() -> anyhow::Result<String> {
    env::var("DATABASE_URL").context("DATABASE_URL must be set")
}

/// Obtains the master prover database URL from the environment variable.
pub fn get_prover_database_url() -> anyhow::Result<String> {
    match env::var("DATABASE_PROVER_URL") {
        Ok(url) => Ok(url),
        Err(_) => get_master_database_url(),
    }
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
pub struct StorageProcessor<Conn = Connection>(Conn);

impl<'a> StorageProcessor<Transaction<'a>> {
    pub async fn commit(self) -> sqlx::Result<()> {
        self.0.commit().await
    }
}

impl From<Connection> for StorageProcessor<Connection> {
    fn from(c: Connection) -> Self {
        Self(c)
    }
}

impl StorageProcessor<Connection> {
    pub async fn establish_connection(
        connect_to_master: bool,
    ) -> anyhow::Result<StorageProcessor<Connection>> {
        let database_url = if connect_to_master {
            get_master_database_url()?
        } else {
            get_replica_database_url()?
        };
        let connection = PgConnection::connect(&database_url)
            .await
            .context("PgConnection::connect()")?;
        Ok(Connection::Direct(connection).into())
    }
}

impl<Conn: Acquire> StorageProcessor<Conn> {
    pub async fn acquire(&mut self) -> ConnectionRef<'_> {
        self.0.acquire().await
    }

    pub async fn start_transaction(&mut self) -> sqlx::Result<StorageProcessor<Transaction<'_>>> {
        Ok(StorageProcessor(self.0.begin().await?))
    }

    pub fn transactions_dal(&mut self) -> TransactionsDal<Conn> {
        TransactionsDal { storage: self }
    }

    pub fn transactions_web3_dal(&mut self) -> TransactionsWeb3Dal<Conn> {
        TransactionsWeb3Dal { storage: self }
    }

    pub fn accounts_dal(&mut self) -> AccountsDal<Conn> {
        AccountsDal { storage: self }
    }

    pub fn blocks_dal(&mut self) -> BlocksDal<Conn> {
        BlocksDal { storage: self }
    }

    pub fn blocks_web3_dal(&mut self) -> BlocksWeb3Dal<Conn> {
        BlocksWeb3Dal { storage: self }
    }

    pub fn eth_sender_dal(&mut self) -> EthSenderDal<Conn> {
        EthSenderDal { storage: self }
    }

    pub fn events_dal(&mut self) -> EventsDal<Conn> {
        EventsDal { storage: self }
    }

    pub fn events_web3_dal(&mut self) -> EventsWeb3Dal<Conn> {
        EventsWeb3Dal { storage: self }
    }

    pub fn storage_dal(&mut self) -> StorageDal<Conn> {
        StorageDal { storage: self }
    }

    pub fn storage_web3_dal(&mut self) -> StorageWeb3Dal<Conn> {
        StorageWeb3Dal { storage: self }
    }

    pub fn storage_logs_dal(&mut self) -> StorageLogsDal<Conn> {
        StorageLogsDal { storage: self }
    }

    pub fn storage_logs_dedup_dal(&mut self) -> StorageLogsDedupDal<Conn> {
        StorageLogsDedupDal { storage: self }
    }

    pub fn tokens_dal(&mut self) -> TokensDal<Conn> {
        TokensDal { storage: self }
    }

    pub fn tokens_web3_dal(&mut self) -> TokensWeb3Dal<Conn> {
        TokensWeb3Dal { storage: self }
    }

    pub fn prover_dal(&mut self) -> ProverDal<Conn> {
        ProverDal { storage: self }
    }

    pub fn witness_generator_dal(&mut self) -> WitnessGeneratorDal<Conn> {
        WitnessGeneratorDal { storage: self }
    }

    pub fn contract_verification_dal(&mut self) -> ContractVerificationDal<Conn> {
        ContractVerificationDal { storage: self }
    }

    pub fn gpu_prover_queue_dal(&mut self) -> GpuProverQueueDal<Conn> {
        GpuProverQueueDal { storage: self }
    }

    pub fn protocol_versions_dal(&mut self) -> ProtocolVersionsDal<Conn> {
        ProtocolVersionsDal { storage: self }
    }

    pub fn protocol_versions_web3_dal(&mut self) -> ProtocolVersionsWeb3Dal<Conn> {
        ProtocolVersionsWeb3Dal { storage: self }
    }

    pub fn fri_witness_generator_dal(&mut self) -> FriWitnessGeneratorDal<Conn> {
        FriWitnessGeneratorDal { storage: self }
    }

    pub fn fri_prover_jobs_dal(&mut self) -> FriProverDal<Conn> {
        FriProverDal { storage: self }
    }

    pub fn sync_dal(&mut self) -> SyncDal<Conn> {
        SyncDal { storage: self }
    }

    pub fn fri_scheduler_dependency_tracker_dal(
        &mut self,
    ) -> FriSchedulerDependencyTrackerDal<Conn> {
        FriSchedulerDependencyTrackerDal { storage: self }
    }

    pub fn proof_generation_dal(&mut self) -> ProofGenerationDal<Conn> {
        ProofGenerationDal { storage: self }
    }

    pub fn fri_gpu_prover_queue_dal(&mut self) -> FriGpuProverQueueDal<Conn> {
        FriGpuProverQueueDal { storage: self }
    }

    pub fn fri_protocol_versions_dal(&mut self) -> FriProtocolVersionsDal<Conn> {
        FriProtocolVersionsDal { storage: self }
    }

    pub fn fri_proof_compressor_dal(&mut self) -> FriProofCompressorDal<Conn> {
        FriProofCompressorDal { storage: self }
    }

    pub fn system_dal(&mut self) -> SystemDal<Conn> {
        SystemDal { storage: self }
    }
}
