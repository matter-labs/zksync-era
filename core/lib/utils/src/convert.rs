use bigdecimal::BigDecimal;
use num::{
    bigint::ToBigInt,
    rational::Ratio,
    traits::{sign::Signed, Pow},
    BigUint,
};
use std::convert::TryInto;
use zksync_basic_types::{Address, H256, U256};

pub fn u256_to_big_decimal(value: U256) -> BigDecimal {
    let ratio = Ratio::new_raw(u256_to_biguint(value), BigUint::from(1u8));
    ratio_to_big_decimal(&ratio, 80)
}

pub fn ratio_to_big_decimal(num: &Ratio<BigUint>, precision: usize) -> BigDecimal {
    let bigint = round_precision_raw_no_div(num, precision)
        .to_bigint()
        .unwrap();
    BigDecimal::new(bigint, precision as i64)
}

pub fn ratio_to_big_decimal_normalized(
    num: &Ratio<BigUint>,
    precision: usize,
    min_precision: usize,
) -> BigDecimal {
    let normalized = ratio_to_big_decimal(num, precision).normalized();
    let min_scaled = normalized.with_scale(min_precision as i64);
    normalized.max(min_scaled)
}

pub fn big_decimal_to_ratio(num: &BigDecimal) -> Result<Ratio<BigUint>, anyhow::Error> {
    let (big_int, exp) = num.as_bigint_and_exponent();
    anyhow::ensure!(!big_int.is_negative(), "BigDecimal should be unsigned");
    let big_uint = big_int.to_biguint().unwrap();
    let ten_pow = BigUint::from(10_u32).pow(exp as u128);
    Ok(Ratio::new(big_uint, ten_pow))
}

fn round_precision_raw_no_div(num: &Ratio<BigUint>, precision: usize) -> BigUint {
    let ten_pow = BigUint::from(10u32).pow(precision);
    (num * ten_pow).round().to_integer()
}

/// Converts `U256` into the corresponding `BigUint` value.
fn u256_to_biguint(value: U256) -> BigUint {
    let mut bytes = [0u8; 32];
    value.to_little_endian(&mut bytes);
    BigUint::from_bytes_le(&bytes)
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

/// Converts u32 into the h256 as BE bytes
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
    use super::*;
    use num::BigInt;
    use std::str::FromStr;

    #[test]
    fn test_ratio_to_big_decimal() {
        let ratio = Ratio::from_integer(BigUint::from(0u32));
        let dec = ratio_to_big_decimal(&ratio, 1);
        assert_eq!(dec.to_string(), "0.0");
        let ratio = Ratio::from_integer(BigUint::from(1234u32));
        let dec = ratio_to_big_decimal(&ratio, 7);
        assert_eq!(dec.to_string(), "1234.0000000");
        // 4 divided by 9 is 0.(4).
        let ratio = Ratio::new(BigUint::from(4u32), BigUint::from(9u32));
        let dec = ratio_to_big_decimal(&ratio, 12);
        assert_eq!(dec.to_string(), "0.444444444444");
        // First 7 decimal digits of pi.
        let ratio = Ratio::new(BigUint::from(52163u32), BigUint::from(16604u32));
        let dec = ratio_to_big_decimal(&ratio, 6);
        assert_eq!(dec.to_string(), "3.141592");
    }

    #[test]
    fn test_ratio_to_big_decimal_normalized() {
        let ratio = Ratio::from_integer(BigUint::from(10u32));
        let dec = ratio_to_big_decimal_normalized(&ratio, 100, 2);
        assert_eq!(dec.to_string(), "10.00");

        // First 7 decimal digits of pi.
        let ratio = Ratio::new(BigUint::from(52163u32), BigUint::from(16604u32));
        let dec = ratio_to_big_decimal_normalized(&ratio, 6, 2);
        assert_eq!(dec.to_string(), "3.141592");

        // 4 divided by 9 is 0.(4).
        let ratio = Ratio::new(BigUint::from(4u32), BigUint::from(9u32));
        let dec = ratio_to_big_decimal_normalized(&ratio, 12, 2);
        assert_eq!(dec.to_string(), "0.444444444444");
    }

    #[test]
    fn test_big_decimal_to_ratio() {
        // Expect unsigned number.
        let dec = BigDecimal::from(-1);
        assert!(big_decimal_to_ratio(&dec).is_err());
        let expected = Ratio::from_integer(BigUint::from(0u32));
        let dec = BigDecimal::from(0);
        let ratio = big_decimal_to_ratio(&dec).unwrap();
        assert_eq!(ratio, expected);
        let expected = Ratio::new(BigUint::from(1234567u32), BigUint::from(10000u32));
        let dec = BigDecimal::from_str("123.4567").unwrap();
        let ratio = big_decimal_to_ratio(&dec).unwrap();
        assert_eq!(ratio, expected);
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
