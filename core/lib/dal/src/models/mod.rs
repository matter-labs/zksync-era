use bigdecimal::{num_bigint::BigUint, BigDecimal};
use zksync_db_connection::error::SqlxContext;
use zksync_types::{ProtocolVersionId, U256};

mod call;
pub mod storage_base_token_ratio;
pub mod storage_block;
pub(crate) mod storage_data_availability;
pub mod storage_eth_tx;
pub mod storage_event;
pub mod storage_log;
pub mod storage_oracle_info;
pub mod storage_protocol_version;
pub mod storage_sync;
pub mod storage_tee_proof;
pub mod storage_transaction;
pub mod storage_verification_request;

pub mod server_notification;
#[cfg(test)]
mod tests;

pub(crate) fn parse_protocol_version(raw: i32) -> sqlx::Result<ProtocolVersionId> {
    u16::try_from(raw)
        .decode_column("protocol_version")?
        .try_into()
        .decode_column("protocol_version")
}

pub(crate) fn u256_to_big_decimal(value: U256) -> BigDecimal {
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
pub(crate) fn bigdecimal_to_u256(value: BigDecimal) -> U256 {
    let bigint = value.with_scale(0).into_bigint_and_exponent().0;
    biguint_to_u256(bigint.to_biguint().unwrap())
}
