use super::AttestationStatus;
use rand::{
    distributions::{Distribution, Standard},
    Rng,
};

impl Distribution<AttestationStatus> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> AttestationStatus {
        AttestationStatus {
            genesis: rng.gen(),
            next_batch_to_attest: rng.gen(),
        }
    }
}

