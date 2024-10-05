use rand::{
    distributions::{Distribution},
    Rng,
};
use zksync_consensus_utils::EncodeDist;
use zksync_utils::{h256_to_u256};
use zksync_l1_contract_interface::i_executor::structures::StoredBatchInfo;

use super::*;

impl Distribution<AttestationStatus> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> AttestationStatus {
        AttestationStatus {
            genesis: rng.gen(),
            next_batch_to_attest: rng.gen(),
        }
    }
}

impl Distribution<GlobalConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> GlobalConfig {
        GlobalConfig {
            genesis: rng.gen(),
            registry_address: Some(rng.gen()),
            seed_peers: self.sample_range(rng).map(|_|(rng.gen(),self.sample(rng))).collect(),
        }
    }
}

impl Distribution<StorageProof> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> StorageProof {
        StorageProof {
            index: self.sample(rng),
            value: rng.gen(),
            merkle_path: self.sample_range(rng).map(|_|rng.gen()).collect(),
        }
    }
}

impl Distribution<BlockDigest> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> BlockDigest {
        BlockDigest {
            txs_rolling_hash: rng.gen(),
            timestamp: self.sample(rng),
        }
    }
}

fn make_info(rng: &mut (impl Rng + ?Sized)) -> StoredBatchInfo {
    StoredBatchInfo {
        batch_number: rng.gen(),
        batch_hash: rng.gen(),
        index_repeated_storage_changes: rng.gen(),
        number_of_layer1_txs: h256_to_u256(rng.gen()),
        priority_operations_hash: rng.gen(),
        l2_logs_tree_root: rng.gen(),
        timestamp: h256_to_u256(rng.gen()),
        commitment: rng.gen(),
    }
}

impl Distribution<BatchProof> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> BatchProof {
        BatchProof {
            info: make_info(rng),
            current_l2_block_info: self.sample(rng), 
            tx_rolling_hash: self.sample(rng),
            l2_block_hash_entry: self.sample(rng), 
    
            initial_hash: rng.gen(),
            blocks: self.sample_collect(rng),
        }
    }
}
