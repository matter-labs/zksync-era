//! Data access layer (DAL) for zkSync Era.

// Linter settings.
#![warn(clippy::cast_lossless)]

pub use sqlx::{types::BigDecimal, Error as SqlxError};
use zksync_db_connection::connection::DbMarker;
pub use zksync_db_connection::{connection::Connection, connection_pool::ConnectionPool};

use crate::{
    basic_witness_input_producer_dal::BasicWitnessInputProducerDal, blocks_dal::BlocksDal,
    blocks_web3_dal::BlocksWeb3Dal, consensus_dal::ConsensusDal,
    contract_verification_dal::ContractVerificationDal, eth_sender_dal::EthSenderDal,
    events_dal::EventsDal, events_web3_dal::EventsWeb3Dal, factory_deps_dal::FactoryDepsDal,
    proof_generation_dal::ProofGenerationDal, protocol_versions_dal::ProtocolVersionsDal,
    protocol_versions_web3_dal::ProtocolVersionsWeb3Dal, pruning_dal::PruningDal,
    snapshot_recovery_dal::SnapshotRecoveryDal, snapshots_creator_dal::SnapshotsCreatorDal,
    snapshots_dal::SnapshotsDal, storage_logs_dal::StorageLogsDal,
    storage_logs_dedup_dal::StorageLogsDedupDal, storage_web3_dal::StorageWeb3Dal,
    sync_dal::SyncDal, system_dal::SystemDal, tokens_dal::TokensDal,
    tokens_web3_dal::TokensWeb3Dal, transactions_dal::TransactionsDal,
    transactions_web3_dal::TransactionsWeb3Dal,
};

pub mod basic_witness_input_producer_dal;
pub mod blocks_dal;
pub mod blocks_web3_dal;
pub mod consensus_dal;
pub mod contract_verification_dal;
pub mod eth_sender_dal;
pub mod events_dal;
pub mod events_web3_dal;
pub mod factory_deps_dal;
mod models;
pub mod proof_generation_dal;
pub mod protocol_versions_dal;
pub mod protocol_versions_web3_dal;
pub mod pruning_dal;
pub mod snapshot_recovery_dal;
pub mod snapshots_creator_dal;
pub mod snapshots_dal;
mod storage_dal;
pub mod storage_logs_dal;
pub mod storage_logs_dedup_dal;
pub mod storage_web3_dal;
pub mod sync_dal;
pub mod system_dal;
pub mod tokens_dal;
pub mod tokens_web3_dal;
pub mod transactions_dal;
pub mod transactions_web3_dal;

pub mod metrics;

#[cfg(test)]
mod tests;

// This module is private and serves as a way to seal the trait.
mod private {
    pub trait Sealed {}
}

// Here we are making the trait sealed, because it should be public to function correctly, but we don't
// want to allow any other downstream implementations of this trait.
pub trait CoreDal<'a>: private::Sealed
where
    Self: 'a,
{
    fn transactions_dal(&mut self) -> TransactionsDal<'_, 'a>;

    fn transactions_web3_dal(&mut self) -> TransactionsWeb3Dal<'_, 'a>;

    fn basic_witness_input_producer_dal(&mut self) -> BasicWitnessInputProducerDal<'_, 'a>;

    fn blocks_dal(&mut self) -> BlocksDal<'_, 'a>;

    fn blocks_web3_dal(&mut self) -> BlocksWeb3Dal<'_, 'a>;

    fn consensus_dal(&mut self) -> ConsensusDal<'_, 'a>;

    fn eth_sender_dal(&mut self) -> EthSenderDal<'_, 'a>;

    fn events_dal(&mut self) -> EventsDal<'_, 'a>;

    fn events_web3_dal(&mut self) -> EventsWeb3Dal<'_, 'a>;

    fn factory_deps_dal(&mut self) -> FactoryDepsDal<'_, 'a>;

    fn storage_web3_dal(&mut self) -> StorageWeb3Dal<'_, 'a>;

    fn storage_logs_dal(&mut self) -> StorageLogsDal<'_, 'a>;

    #[deprecated(note = "Soft-removed in favor of `storage_logs`; don't use")]
    #[allow(deprecated)]
    fn storage_dal(&mut self) -> storage_dal::StorageDal<'_, 'a>;

    fn storage_logs_dedup_dal(&mut self) -> StorageLogsDedupDal<'_, 'a>;

    fn tokens_dal(&mut self) -> TokensDal<'_, 'a>;

    fn tokens_web3_dal(&mut self) -> TokensWeb3Dal<'_, 'a>;

    fn contract_verification_dal(&mut self) -> ContractVerificationDal<'_, 'a>;

    fn protocol_versions_dal(&mut self) -> ProtocolVersionsDal<'_, 'a>;

    fn protocol_versions_web3_dal(&mut self) -> ProtocolVersionsWeb3Dal<'_, 'a>;

    fn sync_dal(&mut self) -> SyncDal<'_, 'a>;

    fn proof_generation_dal(&mut self) -> ProofGenerationDal<'_, 'a>;

    fn system_dal(&mut self) -> SystemDal<'_, 'a>;

    fn snapshots_dal(&mut self) -> SnapshotsDal<'_, 'a>;

    fn snapshots_creator_dal(&mut self) -> SnapshotsCreatorDal<'_, 'a>;

    fn snapshot_recovery_dal(&mut self) -> SnapshotRecoveryDal<'_, 'a>;

    fn pruning_dal(&mut self) -> PruningDal<'_, 'a>;
}

#[derive(Clone, Debug)]
pub struct Core;

// Implement the marker trait for the Core to be able to use it in Connection.
impl DbMarker for Core {}
// Implement the sealed trait for the struct itself.
impl private::Sealed for Connection<'_, Core> {}

impl<'a> CoreDal<'a> for Connection<'a, Core> {
    fn transactions_dal(&mut self) -> TransactionsDal<'_, 'a> {
        TransactionsDal { storage: self }
    }

    fn transactions_web3_dal(&mut self) -> TransactionsWeb3Dal<'_, 'a> {
        TransactionsWeb3Dal { storage: self }
    }

    fn basic_witness_input_producer_dal(&mut self) -> BasicWitnessInputProducerDal<'_, 'a> {
        BasicWitnessInputProducerDal { storage: self }
    }

    fn blocks_dal(&mut self) -> BlocksDal<'_, 'a> {
        BlocksDal { storage: self }
    }

    fn blocks_web3_dal(&mut self) -> BlocksWeb3Dal<'_, 'a> {
        BlocksWeb3Dal { storage: self }
    }

    fn consensus_dal(&mut self) -> ConsensusDal<'_, 'a> {
        ConsensusDal { storage: self }
    }

    fn eth_sender_dal(&mut self) -> EthSenderDal<'_, 'a> {
        EthSenderDal { storage: self }
    }

    fn events_dal(&mut self) -> EventsDal<'_, 'a> {
        EventsDal { storage: self }
    }

    fn events_web3_dal(&mut self) -> EventsWeb3Dal<'_, 'a> {
        EventsWeb3Dal { storage: self }
    }

    fn factory_deps_dal(&mut self) -> FactoryDepsDal<'_, 'a> {
        FactoryDepsDal { storage: self }
    }

    fn storage_web3_dal(&mut self) -> StorageWeb3Dal<'_, 'a> {
        StorageWeb3Dal { storage: self }
    }

    fn storage_logs_dal(&mut self) -> StorageLogsDal<'_, 'a> {
        StorageLogsDal { storage: self }
    }

    fn storage_dal(&mut self) -> storage_dal::StorageDal<'_, 'a> {
        storage_dal::StorageDal { storage: self }
    }

    fn storage_logs_dedup_dal(&mut self) -> StorageLogsDedupDal<'_, 'a> {
        StorageLogsDedupDal { storage: self }
    }

    fn tokens_dal(&mut self) -> TokensDal<'_, 'a> {
        TokensDal { storage: self }
    }

    fn tokens_web3_dal(&mut self) -> TokensWeb3Dal<'_, 'a> {
        TokensWeb3Dal { storage: self }
    }

    fn contract_verification_dal(&mut self) -> ContractVerificationDal<'_, 'a> {
        ContractVerificationDal { storage: self }
    }

    fn protocol_versions_dal(&mut self) -> ProtocolVersionsDal<'_, 'a> {
        ProtocolVersionsDal { storage: self }
    }

    fn protocol_versions_web3_dal(&mut self) -> ProtocolVersionsWeb3Dal<'_, 'a> {
        ProtocolVersionsWeb3Dal { storage: self }
    }

    fn sync_dal(&mut self) -> SyncDal<'_, 'a> {
        SyncDal { storage: self }
    }

    fn proof_generation_dal(&mut self) -> ProofGenerationDal<'_, 'a> {
        ProofGenerationDal { storage: self }
    }

    fn system_dal(&mut self) -> SystemDal<'_, 'a> {
        SystemDal { storage: self }
    }

    fn snapshots_dal(&mut self) -> SnapshotsDal<'_, 'a> {
        SnapshotsDal { storage: self }
    }

    fn snapshots_creator_dal(&mut self) -> SnapshotsCreatorDal<'_, 'a> {
        SnapshotsCreatorDal { storage: self }
    }

    fn snapshot_recovery_dal(&mut self) -> SnapshotRecoveryDal<'_, 'a> {
        SnapshotRecoveryDal { storage: self }
    }

    fn pruning_dal(&mut self) -> PruningDal<'_, 'a> {
        PruningDal { storage: self }
    }
}
