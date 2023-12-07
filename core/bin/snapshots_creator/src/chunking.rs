use std::cmp::min;
use zksync_types::H256;

pub fn get_chunk_hashed_keys_range(chunk_id: u64, chunks_count: u64) -> std::ops::Range<H256> {
    //we don't need whole [u8; 32] range of H256, first two bytes are already enough to evenly divide work
    // as two bytes = 65536 buckets and the chunks count would go in thousands
    let buckets = (u16::MAX as u64) + 1;
    assert!(chunks_count <= buckets);

    //some of the chunks will be exactly this size, some may need to be exactly 1 larger
    let chunk_size = buckets / chunks_count;
    // first (buckets % chunks_count) chunks are bigger by 1, rest are of size chunk_size
    // for instance, if there were 31 buckets and 4 chunks
    // chunk_size would equal 7, first 31 % 4 = 3, first 3 chunks would be of size 8, last one of 7
    // 8 + 8 + 8 + 7 = 31
    let chunk_start = chunk_id * chunk_size + min(chunk_id, buckets % chunks_count);
    let chunk_end = (chunk_id + 1) * chunk_size + min(chunk_id + 1, buckets % chunks_count) - 1;

    let mut start_bytes = (chunk_start as u16).to_be_bytes().to_vec();
    let mut end_bytes = (chunk_end as u16).to_be_bytes().to_vec();

    start_bytes.resize(32, 0);
    end_bytes.resize(32, 0);

    H256::from_slice(&start_bytes)..H256::from_slice(&end_bytes)
}
