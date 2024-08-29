use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use zksync_contracts::consensus as contracts;

use super::AttestationStatus;

impl Distribution<AttestationStatus> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> AttestationStatus {
        AttestationStatus {
            genesis: rng.gen(),
            next_batch_to_attest: rng.gen(),
            consensus_registry_address: Some(contracts::Address::new(rng.gen())),
        }
    }
}
