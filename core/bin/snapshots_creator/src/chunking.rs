use std::cmp::min;
use std::ops;
use zksync_types::{H256, U256};
use zksync_utils::u256_to_h256;

pub fn get_chunk_hashed_keys_range(chunk_id: u64, chunks_count: u64) -> ops::RangeInclusive<H256> {
    assert!(chunks_count > 0);
    let mut stride = U256::MAX / chunks_count;
    let stride_minus_one = if stride < U256::MAX {
        stride += U256::one();
        stride - 1
    } else {
        stride // `stride` is really 1 << 256 == U256::MAX + 1
    };

    let start = stride * chunk_id;
    let (mut end, is_overflow) = stride_minus_one.overflowing_add(start);
    if is_overflow {
        end = U256::MAX;
    }
    u256_to_h256(start)..=u256_to_h256(end)
}
