#![feature(arc_unwrap_or_clone)]
//! Data access layer (DAL) for zkSync Era.

pub use sqlx::{types::BigDecimal, Error as SqlxError};
use zksync_db_connection::StorageProcessor;
use zksync_prover_dal::{
    fri_gpu_prover_queue_dal::FriGpuProverQueueDal,
    fri_proof_compressor_dal::FriProofCompressorDal,
    fri_protocol_versions_dal::FriProtocolVersionsDal, fri_prover_dal::FriProverDal,
    fri_scheduler_dependency_tracker_dal::FriSchedulerDependencyTrackerDal,
    fri_witness_generator_dal::FriWitnessGeneratorDal,
};

pub use crate::connection::ConnectionPool;
use crate::{
    basic_witness_input_producer_dal::BasicWitnessInputProducerDal, blocks_dal::BlocksDal,
    blocks_web3_dal::BlocksWeb3Dal, consensus_dal::ConsensusDal,
    contract_verification_dal::ContractVerificationDal, eth_sender_dal::EthSenderDal,
    events_dal::EventsDal, events_web3_dal::EventsWeb3Dal, factory_deps_dal::FactoryDepsDal,
    proof_generation_dal::ProofGenerationDal, protocol_versions_dal::ProtocolVersionsDal,
    protocol_versions_web3_dal::ProtocolVersionsWeb3Dal,
    snapshot_recovery_dal::SnapshotRecoveryDal, snapshots_creator_dal::SnapshotsCreatorDal,
    snapshots_dal::SnapshotsDal, storage_logs_dal::StorageLogsDal,
    storage_logs_dedup_dal::StorageLogsDedupDal, storage_web3_dal::StorageWeb3Dal,
    sync_dal::SyncDal, system_dal::SystemDal, tokens_dal::TokensDal,
    tokens_web3_dal::TokensWeb3Dal, transactions_dal::TransactionsDal,
    transactions_web3_dal::TransactionsWeb3Dal,
};

#[macro_use]
mod macro_utils;
pub mod basic_witness_input_producer_dal;
pub mod blocks_dal;
pub mod blocks_web3_dal;
pub mod connection;
pub mod consensus_dal;
pub mod contract_verification_dal;
pub mod eth_sender_dal;
pub mod events_dal;
pub mod events_web3_dal;
pub mod factory_deps_dal;
pub mod healthcheck;
mod instrument;
mod metrics;
mod models;
pub mod proof_generation_dal;
pub mod protocol_versions_dal;
pub mod protocol_versions_web3_dal;
pub mod snapshot_recovery_dal;
pub mod snapshots_creator_dal;
pub mod snapshots_dal;
mod storage_dal;
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

#[cfg(test)]
mod tests;

pub struct StorageProcessorWrapper<'a>(pub StorageProcessor<'a>);

impl<'a> StorageProcessorWrapper<'a> {
    pub fn transactions_dal(&mut self) -> TransactionsDal<'_, 'a> {
        TransactionsDal {
            storage: &mut self.0,
        }
    }

    pub fn transactions_web3_dal(&mut self) -> TransactionsWeb3Dal<'_, 'a> {
        TransactionsWeb3Dal {
            storage: &mut self.0,
        }
    }

    pub fn basic_witness_input_producer_dal(&mut self) -> BasicWitnessInputProducerDal<'_, 'a> {
        BasicWitnessInputProducerDal {
            storage: &mut self.0,
        }
    }

    pub fn blocks_dal(&mut self) -> BlocksDal<'_, 'a> {
        BlocksDal {
            storage: &mut self.0,
        }
    }

    pub fn blocks_web3_dal(&mut self) -> BlocksWeb3Dal<'_, 'a> {
        BlocksWeb3Dal {
            storage: &mut self.0,
        }
    }

    pub fn consensus_dal(&mut self) -> ConsensusDal<'_, 'a> {
        ConsensusDal {
            storage: &mut self.0,
        }
    }

    pub fn eth_sender_dal(&mut self) -> EthSenderDal<'_, 'a> {
        EthSenderDal {
            storage: &mut self.0,
        }
    }

    pub fn events_dal(&mut self) -> EventsDal<'_, 'a> {
        EventsDal {
            storage: &mut self.0,
        }
    }

    pub fn events_web3_dal(&mut self) -> EventsWeb3Dal<'_, 'a> {
        EventsWeb3Dal {
            storage: &mut self.0,
        }
    }

    pub fn factory_deps_dal(&mut self) -> FactoryDepsDal<'_, 'a> {
        FactoryDepsDal {
            storage: &mut self.0,
        }
    }

    pub fn storage_web3_dal(&mut self) -> StorageWeb3Dal<'_, 'a> {
        StorageWeb3Dal {
            storage: &mut self.0,
        }
    }

    pub fn storage_logs_dal(&mut self) -> StorageLogsDal<'_, 'a> {
        StorageLogsDal {
            storage: &mut self.0,
        }
    }

    #[deprecated(note = "Soft-removed in favor of `storage_logs`; don't use")]
    #[allow(deprecated)]
    pub fn storage_dal(&mut self) -> storage_dal::StorageDal<'_, 'a> {
        storage_dal::StorageDal {
            storage: &mut self.0,
        }
    }

    pub fn storage_logs_dedup_dal(&mut self) -> StorageLogsDedupDal<'_, 'a> {
        StorageLogsDedupDal {
            storage: &mut self.0,
        }
    }

    pub fn tokens_dal(&mut self) -> TokensDal<'_, 'a> {
        TokensDal {
            storage: &mut self.0,
        }
    }

    pub fn tokens_web3_dal(&mut self) -> TokensWeb3Dal<'_, 'a> {
        TokensWeb3Dal {
            storage: &mut self.0,
        }
    }

    pub fn contract_verification_dal(&mut self) -> ContractVerificationDal<'_, 'a> {
        ContractVerificationDal {
            storage: &mut self.0,
        }
    }

    pub fn protocol_versions_dal(&mut self) -> ProtocolVersionsDal<'_, 'a> {
        ProtocolVersionsDal {
            storage: &mut self.0,
        }
    }

    pub fn protocol_versions_web3_dal(&mut self) -> ProtocolVersionsWeb3Dal<'_, 'a> {
        ProtocolVersionsWeb3Dal {
            storage: &mut self.0,
        }
    }

    pub fn fri_witness_generator_dal(&mut self) -> FriWitnessGeneratorDal<'_, 'a> {
        FriWitnessGeneratorDal {
            storage: &mut self.0,
        }
    }

    pub fn fri_prover_jobs_dal(&mut self) -> FriProverDal<'_, 'a> {
        FriProverDal {
            storage: &mut self.0,
        }
    }

    pub fn sync_dal(&mut self) -> SyncDal<'_, 'a> {
        SyncDal {
            storage: &mut self.0,
        }
    }

    pub fn fri_scheduler_dependency_tracker_dal(
        &mut self,
    ) -> FriSchedulerDependencyTrackerDal<'_, 'a> {
        FriSchedulerDependencyTrackerDal {
            storage: &mut self.0,
        }
    }

    pub fn proof_generation_dal(&mut self) -> ProofGenerationDal<'_, 'a> {
        ProofGenerationDal {
            storage: &mut self.0,
        }
    }

    pub fn fri_gpu_prover_queue_dal(&mut self) -> FriGpuProverQueueDal<'_, 'a> {
        FriGpuProverQueueDal {
            storage: &mut self.0,
        }
    }

    pub fn fri_protocol_versions_dal(&mut self) -> FriProtocolVersionsDal<'_, 'a> {
        FriProtocolVersionsDal {
            storage: &mut self.0,
        }
    }

    pub fn fri_proof_compressor_dal(&mut self) -> FriProofCompressorDal<'_, 'a> {
        FriProofCompressorDal {
            storage: &mut self.0,
        }
    }

    pub fn system_dal(&mut self) -> SystemDal<'_, 'a> {
        SystemDal {
            storage: &mut self.0,
        }
    }

    pub fn snapshots_dal(&mut self) -> SnapshotsDal<'_, 'a> {
        SnapshotsDal {
            storage: &mut self.0,
        }
    }

    pub fn snapshots_creator_dal(&mut self) -> SnapshotsCreatorDal<'_, 'a> {
        SnapshotsCreatorDal {
            storage: &mut self.0,
        }
    }

    pub fn snapshot_recovery_dal(&mut self) -> SnapshotRecoveryDal<'_, 'a> {
        SnapshotRecoveryDal {
            storage: &mut self.0,
        }
    }
}
