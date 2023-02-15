//! Utilities to efficiently pack pubdata (aka storage access slots).
//!
//! Overall the idea is following:
//! If we have a type with at most X bytes, and *most likely* some leading bytes will be
//! zeroes, we can implement a simple variadic length encoding, compressing the type
//! as <length><data>. In case of `uint32` it will allow us to represent number
//! `0x00000022` as `0x0122`. First byte represents the length: 1 byte. Second byte represents
//! the value itself. Knowledge about the type of encoded value is implied: without such a knowledge,
//! we wouldn't be able to parse pubdata at all.
//!
//! Drawbacks of such an approach are the following:
//!
//! - In case of no leading zeroes, we spend one more byte per value. This one is a minor, because as long
//!   as there are more values which *have* leading zeroes, we don't lose anything.
//! - We use complex access keys which may have two components: 32-bit writer ID + 256-bit key, and encoding this
//!   pair may be actually longer than just using the hash (which represents the actual key in the tree).
//!   To overcome this drawback, we will make the value `0xFF` of the length byte special: this value means that instead
//!   of packed key fields we have just an unpacked final hash. At the time of packing we will generate both forms
//!   and choose the shorter one.

use zksync_basic_types::AccountTreeId;
use zksync_utils::h256_to_u256;

use crate::{StorageKey, StorageLog, H256};

const ACCOUNT_TREE_ID_SIZE: usize = 21;
const U256_SIZE: usize = 32;

const fn max_encoded_size(field_size: usize) -> usize {
    field_size + 1
}

pub fn pack_smart_contract(account_id: AccountTreeId, bytecode: Vec<u8>) -> Vec<u8> {
    let max_size = max_encoded_size(ACCOUNT_TREE_ID_SIZE) + bytecode.len();
    let mut packed = Vec::with_capacity(max_size);

    packed.append(&mut encode_account_tree_id(account_id));

    packed
}

pub const fn max_log_size() -> usize {
    // Key is encoded as U168 + U256, value is U256.
    max_encoded_size(ACCOUNT_TREE_ID_SIZE) + max_encoded_size(U256_SIZE) * 2
}

pub fn pack_storage_log<F>(log: &StorageLog, _hash_key: F) -> Vec<u8>
where
    F: FnOnce(&StorageKey) -> Vec<u8>,
{
    pack_storage_log_packed_old(log)
}

/// Does not pack anything; just encodes account address, storage key and storage value as bytes.
/// Encoding is exactly 20 + 32 + 32 bytes in size.
pub fn pack_storage_log_unpacked(log: &StorageLog) -> Vec<u8> {
    log.to_bytes()
}

/// Packs address, storage key and storage value as 3 separate values.
/// Encoding is at most 21 + 33 + 33 bytes in size.
pub fn pack_storage_log_packed_old(log: &StorageLog) -> Vec<u8> {
    let mut packed_log = Vec::with_capacity(max_log_size());

    packed_log.append(&mut encode_key(&log.key, |key| {
        key.key().to_fixed_bytes().to_vec()
    }));
    packed_log.append(&mut encode_h256(log.value));

    packed_log
}

/// Computes the hash of the (address, storage key) and packs the storage value.
/// Encoding is at most 32 + 33 bytes in size.
pub fn pack_storage_log_packed_new(log: &StorageLog) -> Vec<u8> {
    let mut packed_log = Vec::with_capacity(max_log_size());

    packed_log.extend_from_slice(&log.key.hashed_key().to_fixed_bytes());
    packed_log.append(&mut encode_h256(log.value));

    packed_log
}

fn encode_key<F>(key: &StorageKey, hash_key: F) -> Vec<u8>
where
    F: FnOnce(&StorageKey) -> Vec<u8>,
{
    let mut key_hash = Vec::with_capacity(max_encoded_size(U256_SIZE));
    key_hash.push(0xFFu8);
    key_hash.append(&mut hash_key(key));

    let encoded_storage_key = encode_h256(*key.key());

    let mut storage_key_part = if encoded_storage_key.len() <= key_hash.len() {
        encoded_storage_key
    } else {
        key_hash
    };

    let mut encoded_key =
        Vec::with_capacity(max_encoded_size(U256_SIZE) + max_encoded_size(ACCOUNT_TREE_ID_SIZE));
    encoded_key.append(&mut encode_account_tree_id(*key.account()));
    encoded_key.append(&mut storage_key_part);

    encoded_key
}

fn encode_account_tree_id(val: AccountTreeId) -> Vec<u8> {
    let mut result = vec![0; 21];
    result[0] = 20;
    result[1..].copy_from_slice(&val.to_fixed_bytes());

    result
}

fn encode_h256(val: H256) -> Vec<u8> {
    let val = h256_to_u256(val);
    let leading_zero_bytes = (val.leading_zeros() / 8) as usize;
    let result_vec_length = (1 + U256_SIZE) - leading_zero_bytes;
    let val_len = result_vec_length - 1;
    let mut result = vec![0; result_vec_length];

    let mut val_bytes = [0u8; 32];
    val.to_big_endian(&mut val_bytes);

    result[0] = val_len as u8;
    if val_len > 0 {
        result[1..].copy_from_slice(&val_bytes[leading_zero_bytes..]);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccountTreeId, Address, U256};
    use zksync_utils::{u256_to_h256, u64_to_h256};

    fn check_encoding<T, F>(f: F, input: impl Into<T>, output: &str)
    where
        F: Fn(T) -> Vec<u8>,
    {
        let output = hex::decode(output).unwrap();
        assert_eq!(f(input.into()), output);
    }

    #[test]
    fn u256_encoding() {
        let test_vector = vec![
            (u64_to_h256(0x00_00_00_00_u64), "00"),
            (u64_to_h256(0x00_00_00_01_u64), "0101"),
            (u64_to_h256(0x00_00_00_FF_u64), "01FF"),
            (u64_to_h256(0x00_00_01_00_u64), "020100"),
            (u64_to_h256(0x10_01_00_00_u64), "0410010000"),
            (u64_to_h256(0xFF_FF_FF_FF_u64), "04FFFFFFFF"),
        ];

        for (input, output) in test_vector {
            check_encoding(encode_h256, input, output);
        }

        let max = u256_to_h256(U256::max_value());
        check_encoding(
            encode_h256,
            max,
            "20FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        );

        let one_leading_zero_bit = U256::max_value() >> 1;
        assert_eq!(one_leading_zero_bit.leading_zeros(), 1);
        let one_leading_zero_bit = u256_to_h256(one_leading_zero_bit);
        check_encoding(
            encode_h256,
            one_leading_zero_bit,
            "207FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        );

        let one_leading_zero_byte = U256::max_value() >> 8;
        assert_eq!(one_leading_zero_byte.leading_zeros(), 8);
        let one_leading_zero_byte = u256_to_h256(one_leading_zero_byte);
        check_encoding(
            encode_h256,
            one_leading_zero_byte,
            "1FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        );
    }

    fn pseudo_hash(key: &StorageKey) -> Vec<u8> {
        // Just return something 32-bit long.
        key.key().to_fixed_bytes().to_vec()
    }

    #[test]
    fn key_encoding() {
        // Raw key must be encoded in the compressed form, because hash will be longer.
        let short_key = StorageKey::new(
            AccountTreeId::new(Address::from_slice(&[0x0A; 20])),
            u64_to_h256(0xDEAD_F00D_u64),
        );
        // `140A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A` is encoding of `AccountTreeId`.
        // 0x14 is number of bytes that should be decoded.
        let expected_output = "140A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A0A04DEADF00D";

        check_encoding(
            |key| encode_key(key, pseudo_hash),
            &short_key,
            expected_output,
        );
    }

    /// Compares multiple packing approaches we have. Does not assert anything.
    /// If you see this test and know that the packing algorithm is already chosen and used in
    /// production, please remvoe this test. Also, remove the similar tests in the `runtime_context`
    /// module of `zksync_state` crate.
    #[test]
    fn pack_log_comparison() {
        let log1 = StorageLog::new_write_log(
            StorageKey::new(AccountTreeId::new(Address::random()), H256::random()),
            H256::random(),
        );
        let log2 = StorageLog::new_write_log(
            StorageKey::new(
                AccountTreeId::new(Address::repeat_byte(0x11)),
                H256::repeat_byte(0x22),
            ),
            H256::repeat_byte(0x33),
        );
        let log3 = StorageLog::new_write_log(
            StorageKey::new(
                AccountTreeId::new(Address::repeat_byte(0x11)),
                H256::from_low_u64_be(0x01),
            ),
            H256::from_low_u64_be(0x02),
        );
        let log4 = StorageLog::new_write_log(
            StorageKey::new(
                AccountTreeId::new(Address::repeat_byte(0x11)),
                H256::repeat_byte(0x22),
            ),
            H256::from_low_u64_be(0x02),
        );

        let test_vector = &[
            (log1, "Random values"),
            (log2, "32-byte key/value"),
            (log3, "1-byte key/value"),
            (log4, "32-byte key/1-byte value"),
        ];

        for (log, description) in test_vector {
            let no_packing = pack_storage_log_unpacked(log);
            let old_packing = pack_storage_log_packed_old(log);
            let new_packing = pack_storage_log_packed_new(log);

            println!("Packing {}", description);
            println!("No packing: {} bytes", no_packing.len());
            println!("Old packing: {} bytes", old_packing.len());
            println!("New packing: {} bytes", new_packing.len());
            println!("-----------------------");
        }
    }
}
