use rand::{distributions::Distribution, Rng};
use zksync_consensus_utils::EncodeDist;

use super::*;

impl Distribution<BlockMetadata> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> BlockMetadata {
        BlockMetadata {
            payload_hash: rng.gen(),
        }
    }
}

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
            seed_peers: self
                .sample_range(rng)
                .map(|_| (rng.gen(), self.sample(rng)))
                .collect(),
        }
    }
}
