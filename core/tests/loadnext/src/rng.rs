use std::convert::TryInto;

use rand::{rngs::SmallRng, seq::SliceRandom, thread_rng, RngCore, SeedableRng};

use crate::{all::AllWeighted, sdk::web3::keccak256};

// SmallRng seed type is [u8; 32].
const SEED_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub struct LoadtestRng {
    pub seed: [u8; SEED_SIZE],
    rng: SmallRng,
}

impl LoadtestRng {
    pub fn new_generic(seed_hex: Option<String>) -> Self {
        let seed: [u8; SEED_SIZE] = seed_hex
            .map(|seed_str| {
                let mut output = [0u8; SEED_SIZE];
                let decoded_seed = hex::decode(seed_str).expect("Incorrect seed hex");
                output.copy_from_slice(decoded_seed.as_ref());
                output
            })
            .unwrap_or_else(|| {
                let rng = &mut thread_rng();
                let mut output = [0u8; SEED_SIZE];
                rng.fill_bytes(&mut output);

                output
            });

        let rng = SmallRng::from_seed(seed);

        Self { seed, rng }
    }

    pub fn seed_hex(&self) -> String {
        hex::encode(self.seed)
    }

    pub fn derive(&self, private_key_bytes: [u8; 32]) -> Self {
        // We chain the current seed bytes and the Ethereum private key together,
        // and then calculate the hash of this data.
        // This way we obtain a derived seed, unique for each wallet, which will result in
        // a unique set of operations for each account.
        let input_bytes: Vec<u8> = self.seed.into_iter().chain(private_key_bytes).collect();
        let data_hash = keccak256(&input_bytes);
        let new_seed = data_hash[..SEED_SIZE].try_into().unwrap();

        let rng = SmallRng::from_seed(new_seed);
        Self {
            seed: new_seed,
            rng,
        }
    }
}

impl RngCore for LoadtestRng {
    fn next_u32(&mut self) -> u32 {
        self.rng.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.rng.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.rng.try_fill_bytes(dest)
    }
}

pub trait Random {
    fn random(rng: &mut LoadtestRng) -> Self;
}

pub trait WeightedRandom {
    fn random(rng: &mut LoadtestRng) -> Self;
}

impl<V> WeightedRandom for V
where
    V: 'static + AllWeighted + Sized + Copy,
{
    fn random(rng: &mut LoadtestRng) -> Self {
        V::all_weighted()
            .choose_weighted(rng, |item| item.1)
            .unwrap()
            .0
    }
}
