use std::ops;

use zksync_types::{H256, U256};
use zksync_utils::u256_to_h256;

pub(crate) fn get_chunk_hashed_keys_range(
    chunk_id: u64,
    chunk_count: u64,
) -> ops::RangeInclusive<H256> {
    assert!(chunk_count > 0);
    let mut stride = U256::MAX / chunk_count;
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

#[cfg(test)]
mod tests {
    use zksync_utils::h256_to_u256;

    use super::*;

    #[test]
    fn chunking_is_correct() {
        for chunks_count in (2..10).chain([42, 256, 500, 1_001, 12_345]) {
            println!("Testing chunks_count={chunks_count}");
            let chunked_ranges: Vec<_> = (0..chunks_count)
                .map(|chunk_id| get_chunk_hashed_keys_range(chunk_id, chunks_count))
                .collect();

            assert_eq!(*chunked_ranges[0].start(), H256::zero());
            assert_eq!(
                *chunked_ranges.last().unwrap().end(),
                H256::repeat_byte(0xff)
            );
            for window in chunked_ranges.windows(2) {
                let [prev_chunk, next_chunk] = window else {
                    unreachable!();
                };
                assert_eq!(
                    h256_to_u256(*prev_chunk.end()) + 1,
                    h256_to_u256(*next_chunk.start())
                );
            }

            let chunk_sizes: Vec<_> = chunked_ranges
                .iter()
                .map(|chunk| h256_to_u256(*chunk.end()) - h256_to_u256(*chunk.start()) + 1)
                .collect();

            // Check that chunk sizes are roughly equal. Due to how chunks are constructed, the sizes
            // of all chunks except for the last one are the same, and the last chunk size may be slightly smaller;
            // the difference in sizes is lesser than the number of chunks.
            let min_chunk_size = chunk_sizes.iter().copied().min().unwrap();
            let max_chunk_size = chunk_sizes.iter().copied().max().unwrap();
            assert!(max_chunk_size - min_chunk_size < U256::from(chunks_count));
        }
    }
}
