use std::convert::TryInto;

use bigdecimal::BigDecimal;
use num::BigUint;
use zksync_basic_types::{Address, H256, U256};

pub fn u256_to_big_decimal(value: U256) -> BigDecimal {
    let mut u32_digits = vec![0_u32; 8];
    // `u64_digit`s from `U256` are little-endian
    for (i, &u64_digit) in value.0.iter().enumerate() {
        u32_digits[2 * i] = u64_digit as u32;
        u32_digits[2 * i + 1] = (u64_digit >> 32) as u32;
    }
    let value = BigUint::new(u32_digits);
    BigDecimal::new(value.into(), 0)
}

/// Converts `BigUint` value into the corresponding `U256` value.
fn biguint_to_u256(value: BigUint) -> U256 {
    let bytes = value.to_bytes_le();
    U256::from_little_endian(&bytes)
}

/// Converts `BigDecimal` value into the corresponding `U256` value.
pub fn bigdecimal_to_u256(value: BigDecimal) -> U256 {
    let bigint = value.with_scale(0).into_bigint_and_exponent().0;
    biguint_to_u256(bigint.to_biguint().unwrap())
}

fn ensure_chunkable(bytes: &[u8]) {
    assert!(
        bytes.len() % 32 == 0,
        "Bytes must be divisible by 32 to split into chunks"
    );
}

pub fn h256_to_u256(num: H256) -> U256 {
    U256::from_big_endian(num.as_bytes())
}

pub fn address_to_h256(address: &Address) -> H256 {
    let mut buffer = [0u8; 32];
    buffer[12..].copy_from_slice(address.as_bytes());
    H256(buffer)
}

pub fn address_to_u256(address: &Address) -> U256 {
    h256_to_u256(address_to_h256(address))
}

pub fn bytes_to_chunks(bytes: &[u8]) -> Vec<[u8; 32]> {
    ensure_chunkable(bytes);
    bytes
        .chunks(32)
        .map(|el| {
            let mut chunk = [0u8; 32];
            chunk.copy_from_slice(el);
            chunk
        })
        .collect()
}

pub fn be_chunks_to_h256_words(chunks: Vec<[u8; 32]>) -> Vec<H256> {
    chunks.into_iter().map(|el| H256::from_slice(&el)).collect()
}

pub fn bytes_to_be_words(vec: Vec<u8>) -> Vec<U256> {
    ensure_chunkable(&vec);
    vec.chunks(32).map(U256::from_big_endian).collect()
}

pub fn be_words_to_bytes(words: &[U256]) -> Vec<u8> {
    words
        .iter()
        .flat_map(|w| {
            let mut bytes = [0u8; 32];
            w.to_big_endian(&mut bytes);
            bytes
        })
        .collect()
}

pub fn u256_to_h256(num: U256) -> H256 {
    let mut bytes = [0u8; 32];
    num.to_big_endian(&mut bytes);
    H256::from_slice(&bytes)
}

/// Converts `U256` value into the Address
pub fn u256_to_account_address(value: &U256) -> Address {
    let mut bytes = [0u8; 32];
    value.to_big_endian(&mut bytes);

    Address::from_slice(&bytes[12..])
}

/// Converts `H256` value into the Address
pub fn h256_to_account_address(value: &H256) -> Address {
    Address::from_slice(&value.as_bytes()[12..])
}

pub fn be_bytes_to_safe_address(bytes: &[u8]) -> Option<Address> {
    if bytes.len() < 20 {
        return None;
    }

    let (zero_bytes, address_bytes) = bytes.split_at(bytes.len() - 20);

    if zero_bytes.iter().any(|b| *b != 0) {
        None
    } else {
        Some(Address::from_slice(address_bytes))
    }
}

/// Converts `h256` value as BE into the u32
pub fn h256_to_u32(value: H256) -> u32 {
    let be_u32_bytes: [u8; 4] = value[28..].try_into().unwrap();
    u32::from_be_bytes(be_u32_bytes)
}

/// Converts u32 into the H256 as BE bytes
pub fn u32_to_h256(value: u32) -> H256 {
    let mut result = [0u8; 32];
    result[28..].copy_from_slice(&value.to_be_bytes());
    H256(result)
}

/// Converts `U256` value into bytes array
pub fn u256_to_bytes_be(value: &U256) -> Vec<u8> {
    let mut bytes = vec![0u8; 32];
    value.to_big_endian(bytes.as_mut_slice());
    bytes
}

#[cfg(test)]
mod test {
    use num::BigInt;
    use rand::{rngs::StdRng, Rng, SeedableRng};

    use super::*;

    #[test]
    fn test_u256_to_bigdecimal() {
        const RNG_SEED: u64 = 123;

        let mut rng = StdRng::seed_from_u64(RNG_SEED);
        // Small values.
        for _ in 0..10_000 {
            let value: u64 = rng.gen();
            let expected = BigDecimal::from(value);
            assert_eq!(u256_to_big_decimal(value.into()), expected);
        }

        // Arbitrary values
        for _ in 0..10_000 {
            let u64_digits: [u64; 4] = rng.gen();
            let value = u64_digits
                .iter()
                .enumerate()
                .map(|(i, &digit)| U256::from(digit) << (i * 64))
                .fold(U256::zero(), |acc, x| acc + x);
            let expected_value = u64_digits
                .iter()
                .enumerate()
                .map(|(i, &digit)| BigInt::from(digit) << (i * 64))
                .fold(BigInt::from(0), |acc, x| acc + x);
            assert_eq!(
                u256_to_big_decimal(value),
                BigDecimal::new(expected_value, 0)
            );
        }
    }

    #[test]
    fn test_bigdecimal_to_u256() {
        let value = BigDecimal::from(100u32);
        let expected = U256::from(100u32);
        assert_eq!(bigdecimal_to_u256(value), expected);

        let value = BigDecimal::new(BigInt::from(100), -2);
        let expected = U256::from(10000u32);
        assert_eq!(bigdecimal_to_u256(value), expected);
    }
}
