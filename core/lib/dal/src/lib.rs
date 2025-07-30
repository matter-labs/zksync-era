//! Data access layer (DAL) for ZKsync Era.

// Linter settings.
#![warn(clippy::cast_lossless)]

pub use sqlx::{types::BigDecimal, Error as SqlxError};
use zksync_db_connection::connection::DbMarker;
pub use zksync_db_connection::{
    connection::{Connection, IsolationLevel},
    connection_pool::{ConnectionPool, ConnectionPoolBuilder},
    error::{DalError, DalResult},
};

use crate::{
    base_token_dal::BaseTokenDal, blocks_dal::BlocksDal, blocks_web3_dal::BlocksWeb3Dal,
    consensus_dal::ConsensusDal, contract_verification_dal::ContractVerificationDal,
    custom_genesis_export_dal::CustomGenesisExportDal, data_availability_dal::DataAvailabilityDal,
    eth_sender_dal::EthSenderDal, eth_watcher_dal::EthWatcherDal,
    etherscan_verification_dal::EtherscanVerificationDal, events_dal::EventsDal,
    events_web3_dal::EventsWeb3Dal, factory_deps_dal::FactoryDepsDal,
    interop_roots_dal::InteropRootDal, proof_generation_dal::ProofGenerationDal,
    protocol_versions_dal::ProtocolVersionsDal,
    protocol_versions_web3_dal::ProtocolVersionsWeb3Dal, pruning_dal::PruningDal,
    server_notifications::ServerNotificationsDal, snapshot_recovery_dal::SnapshotRecoveryDal,
    snapshots_creator_dal::SnapshotsCreatorDal, snapshots_dal::SnapshotsDal,
    storage_logs_dal::StorageLogsDal, storage_logs_dedup_dal::StorageLogsDedupDal,
    storage_web3_dal::StorageWeb3Dal, sync_dal::SyncDal, system_dal::SystemDal,
    tee_proof_generation_dal::TeeProofGenerationDal, tokens_dal::TokensDal,
    tokens_web3_dal::TokensWeb3Dal, transactions_dal::TransactionsDal,
    transactions_web3_dal::TransactionsWeb3Dal, vm_runner_dal::VmRunnerDal,
};

pub mod base_token_dal;
pub mod blocks_dal;
pub mod blocks_web3_dal;
pub mod consensus;
pub mod consensus_dal;
pub mod contract_verification_dal;
pub mod custom_genesis_export_dal;
mod data_availability_dal;
pub mod eth_sender_dal;
pub mod eth_watcher_dal;
pub mod etherscan_verification_dal;
pub mod events_dal;
pub mod events_web3_dal;
pub mod factory_deps_dal;
pub mod helpers;
pub mod interop_roots_dal;
pub mod metrics;
mod models;
#[cfg(feature = "node_framework")]
pub mod node;
pub mod proof_generation_dal;
pub mod protocol_versions_dal;
pub mod protocol_versions_web3_dal;
pub mod pruning_dal;
mod server_notifications;
pub mod snapshot_recovery_dal;
pub mod snapshots_creator_dal;
pub mod snapshots_dal;
pub mod storage_logs_dal;
pub mod storage_logs_dedup_dal;
pub mod storage_web3_dal;
pub mod sync_dal;
pub mod system_dal;
pub mod tee_proof_generation_dal;
pub mod tokens_dal;
pub mod tokens_web3_dal;
pub mod transactions_dal;
pub mod transactions_web3_dal;
pub mod vm_runner_dal;

pub mod external_node_config_dal;

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

    fn blocks_dal(&mut self) -> BlocksDal<'_, 'a>;

    fn blocks_web3_dal(&mut self) -> BlocksWeb3Dal<'_, 'a>;

    fn consensus_dal(&mut self) -> ConsensusDal<'_, 'a>;

    fn eth_sender_dal(&mut self) -> EthSenderDal<'_, 'a>;

    fn events_dal(&mut self) -> EventsDal<'_, 'a>;

    fn events_web3_dal(&mut self) -> EventsWeb3Dal<'_, 'a>;

    fn factory_deps_dal(&mut self) -> FactoryDepsDal<'_, 'a>;

    fn storage_web3_dal(&mut self) -> StorageWeb3Dal<'_, 'a>;

    fn storage_logs_dal(&mut self) -> StorageLogsDal<'_, 'a>;

    fn storage_logs_dedup_dal(&mut self) -> StorageLogsDedupDal<'_, 'a>;

    fn tokens_dal(&mut self) -> TokensDal<'_, 'a>;

    fn tokens_web3_dal(&mut self) -> TokensWeb3Dal<'_, 'a>;

    fn contract_verification_dal(&mut self) -> ContractVerificationDal<'_, 'a>;

    fn etherscan_verification_dal(&mut self) -> EtherscanVerificationDal<'_, 'a>;

    fn protocol_versions_dal(&mut self) -> ProtocolVersionsDal<'_, 'a>;

    fn interop_root_dal(&mut self) -> InteropRootDal<'_, 'a>;

    fn protocol_versions_web3_dal(&mut self) -> ProtocolVersionsWeb3Dal<'_, 'a>;

    fn sync_dal(&mut self) -> SyncDal<'_, 'a>;

    fn proof_generation_dal(&mut self) -> ProofGenerationDal<'_, 'a>;

    fn tee_proof_generation_dal(&mut self) -> TeeProofGenerationDal<'_, 'a>;

    fn system_dal(&mut self) -> SystemDal<'_, 'a>;

    fn snapshots_dal(&mut self) -> SnapshotsDal<'_, 'a>;

    fn snapshots_creator_dal(&mut self) -> SnapshotsCreatorDal<'_, 'a>;

    fn snapshot_recovery_dal(&mut self) -> SnapshotRecoveryDal<'_, 'a>;

    fn pruning_dal(&mut self) -> PruningDal<'_, 'a>;

    fn data_availability_dal(&mut self) -> DataAvailabilityDal<'_, 'a>;

    fn vm_runner_dal(&mut self) -> VmRunnerDal<'_, 'a>;

    fn base_token_dal(&mut self) -> BaseTokenDal<'_, 'a>;

    fn eth_watcher_dal(&mut self) -> EthWatcherDal<'_, 'a>;

    fn custom_genesis_export_dal(&mut self) -> CustomGenesisExportDal<'_, 'a>;

    fn server_notifications_dal(&mut self) -> ServerNotificationsDal<'_, 'a>;
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

    fn etherscan_verification_dal(&mut self) -> EtherscanVerificationDal<'_, 'a> {
        EtherscanVerificationDal { storage: self }
    }

    fn protocol_versions_dal(&mut self) -> ProtocolVersionsDal<'_, 'a> {
        ProtocolVersionsDal { storage: self }
    }

    fn interop_root_dal(&mut self) -> InteropRootDal<'_, 'a> {
        InteropRootDal { storage: self }
    }

    fn protocol_versions_web3_dal(&mut self) -> ProtocolVersionsWeb3Dal<'_, 'a> {
        ProtocolVersionsWeb3Dal { storage: self }
    }

    fn server_notifications_dal(&mut self) -> ServerNotificationsDal<'_, 'a> {
        ServerNotificationsDal { storage: self }
    }

    fn sync_dal(&mut self) -> SyncDal<'_, 'a> {
        SyncDal { storage: self }
    }

    fn proof_generation_dal(&mut self) -> ProofGenerationDal<'_, 'a> {
        ProofGenerationDal { storage: self }
    }

    fn tee_proof_generation_dal(&mut self) -> TeeProofGenerationDal<'_, 'a> {
        TeeProofGenerationDal { storage: self }
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

    fn data_availability_dal(&mut self) -> DataAvailabilityDal<'_, 'a> {
        DataAvailabilityDal { storage: self }
    }

    fn vm_runner_dal(&mut self) -> VmRunnerDal<'_, 'a> {
        VmRunnerDal { storage: self }
    }

    fn base_token_dal(&mut self) -> BaseTokenDal<'_, 'a> {
        BaseTokenDal { storage: self }
    }

    fn eth_watcher_dal(&mut self) -> EthWatcherDal<'_, 'a> {
        EthWatcherDal { storage: self }
    }

    fn custom_genesis_export_dal(&mut self) -> CustomGenesisExportDal<'_, 'a> {
        CustomGenesisExportDal { storage: self }
    }
}
