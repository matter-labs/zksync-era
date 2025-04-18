use std::collections::HashMap;

use zksync_types::{
    bytecode::{validate_bytecode, BytecodeHash, InvalidBytecodeError},
    ethabi::{self, Token},
    Address, H256, U256,
};

use crate::interface::CompressedBytecodeInfo;

pub(crate) fn be_chunks_to_h256_words(chunks: Vec<[u8; 32]>) -> Vec<H256> {
    chunks.into_iter().map(|el| H256::from_slice(&el)).collect()
}

pub(crate) fn be_words_to_bytes(words: &[U256]) -> Vec<u8> {
    words
        .iter()
        .flat_map(|w| {
            let mut bytes = [0u8; 32];
            w.to_big_endian(&mut bytes);
            bytes
        })
        .collect()
}

pub fn bytes_to_be_words(bytes: &[u8]) -> Vec<U256> {
    assert_eq!(
        bytes.len() % 32,
        0,
        "Bytes must be divisible by 32 to split into chunks"
    );
    bytes.chunks(32).map(U256::from_big_endian).collect()
}

pub(crate) fn be_bytes_to_safe_address(bytes: &[u8]) -> Option<Address> {
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

pub(crate) fn bytecode_len_in_words(bytecode_hash: &H256) -> u16 {
    let bytes = bytecode_hash.as_bytes();
    u16::from_be_bytes([bytes[2], bytes[3]])
}

pub(crate) fn bytecode_len_in_bytes(bytecode_hash: &H256) -> u32 {
    u32::from(bytecode_len_in_words(bytecode_hash)) * 32
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FailedToCompressBytecodeError {
    #[error("Number of unique 8-bytes bytecode chunks exceed the limit of 2^16 - 1")]
    DictionaryOverflow,
    #[error("Bytecode is invalid: {0}")]
    InvalidBytecode(#[from] InvalidBytecodeError),
}

/// Implements, a simple compression algorithm for the bytecode.
fn compress_to_bytes(code: &[u8]) -> Result<Vec<u8>, FailedToCompressBytecodeError> {
    validate_bytecode(code)?;

    // Statistic is a hash map of values (number of occurrences, first occurrence position),
    // this is needed to ensure that the determinism during sorting of the statistic, i.e.
    // each element will have unique first occurrence position
    let mut statistic: HashMap<u64, (usize, usize)> = HashMap::new();
    let mut dictionary: HashMap<u64, u16> = HashMap::new();
    let mut encoded_data: Vec<u8> = Vec::new();

    // Split original bytecode into 8-byte chunks.
    for (position, chunk_bytes) in code.chunks(8).enumerate() {
        // It is safe to unwrap here, because each chunk is exactly 8 bytes, since
        // valid bytecodes are divisible by 8.
        let chunk = u64::from_be_bytes(chunk_bytes.try_into().unwrap());

        // Count the number of occurrences of each chunk.
        statistic.entry(chunk).or_insert((0, position)).0 += 1;
    }

    let mut statistic_sorted_by_value: Vec<_> = statistic.into_iter().collect::<Vec<_>>();
    statistic_sorted_by_value.sort_by_key(|x| x.1);

    // The dictionary size is limited by 2^16 - 1,
    if statistic_sorted_by_value.len() > (u16::MAX as usize) {
        return Err(FailedToCompressBytecodeError::DictionaryOverflow);
    }

    // Fill the dictionary with the most popular chunks.
    // The most popular chunks will be encoded with the smallest indexes, so that
    // the 255 most popular chunks will be encoded with one zero byte.
    // And the encoded data will be filled with more zeros, so
    // the calldata that will be sent to L1 will be cheaper.
    for (chunk, _) in statistic_sorted_by_value.iter().rev() {
        dictionary.insert(*chunk, dictionary.len() as u16);
    }

    for chunk_bytes in code.chunks(8) {
        // It is safe to unwrap here, because each chunk is exactly 8 bytes, since
        // valid bytecodes are divisible by 8.
        let chunk = u64::from_be_bytes(chunk_bytes.try_into().unwrap());

        // Add the index of the chunk to the encoded data.
        encoded_data.extend(dictionary.get(&chunk).unwrap().to_be_bytes());
    }

    // Prepare the raw compressed bytecode in the following format:
    // - 2 bytes: the length of the dictionary (N)
    // - N bytes: packed dictionary bytes
    // - remaining bytes: packed encoded data bytes

    let mut compressed: Vec<u8> = Vec::new();
    compressed.extend((dictionary.len() as u16).to_be_bytes());

    let mut entries: Vec<_> = dictionary.into_iter().map(|(k, v)| (v, k)).collect();
    entries.sort_unstable();
    for (_, chunk) in entries {
        compressed.extend(chunk.to_be_bytes());
    }
    compressed.extend(encoded_data);
    Ok(compressed)
}

pub(crate) fn compress(
    bytecode: Vec<u8>,
) -> Result<CompressedBytecodeInfo, FailedToCompressBytecodeError> {
    Ok(CompressedBytecodeInfo {
        compressed: compress_to_bytes(&bytecode)?,
        original: bytecode,
    })
}

pub(crate) fn encode_call(bytecode: &CompressedBytecodeInfo) -> Vec<u8> {
    let mut bytecode_hash = BytecodeHash::for_bytecode(&bytecode.original)
        .value()
        .as_bytes()
        .to_vec();
    let empty_cell = [0_u8; 32];
    bytecode_hash.extend_from_slice(&empty_cell);

    let bytes_encoded = ethabi::encode(&[
        Token::Bytes(bytecode.original.clone()),
        Token::Bytes(bytecode.compressed.clone()),
    ]);
    bytecode_hash.extend_from_slice(&bytes_encoded);
    bytecode_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decompress_bytecode(raw_compressed_bytecode: &[u8]) -> Vec<u8> {
        let mut decompressed: Vec<u8> = Vec::new();
        let mut dictionary: Vec<u64> = Vec::new();

        let dictionary_len = u16::from_be_bytes(raw_compressed_bytecode[0..2].try_into().unwrap());
        for index in 0..dictionary_len {
            let chunk = u64::from_be_bytes(
                raw_compressed_bytecode[2 + index as usize * 8..10 + index as usize * 8]
                    .try_into()
                    .unwrap(),
            );
            dictionary.push(chunk);
        }

        let encoded_data = &raw_compressed_bytecode[2 + dictionary_len as usize * 8..];
        for index_bytes in encoded_data.chunks(2) {
            let index = u16::from_be_bytes(index_bytes.try_into().unwrap());

            let chunk = dictionary[index as usize];
            decompressed.extend(chunk.to_be_bytes());
        }

        decompressed
    }

    #[test]
    fn bytecode_compression() {
        let example_code = hex::decode("000200000000000200010000000103550000006001100270000000150010019d0000000101200190000000080000c13d0000000001000019004e00160000040f0000000101000039004e00160000040f0000001504000041000000150510009c000000000104801900000040011002100000000001310019000000150320009c0000000002048019000000600220021000000000012100190000004f0001042e000000000100001900000050000104300000008002000039000000400020043f0000000002000416000000000110004c000000240000613d000000000120004c0000004d0000c13d000000200100003900000100001004430000012000000443000001000100003900000040020000390000001d03000041004e000a0000040f000000000120004c0000004d0000c13d0000000001000031000000030110008c0000004d0000a13d0000000101000367000000000101043b0000001601100197000000170110009c0000004d0000c13d0000000101000039000000000101041a0000000202000039000000000202041a000000400300043d00000040043000390000001805200197000000000600041a0000000000540435000000180110019700000020043000390000000000140435000000a0012002700000001901100197000000600430003900000000001404350000001a012001980000001b010000410000000001006019000000b8022002700000001c02200197000000000121019f0000008002300039000000000012043500000018016001970000000000130435000000400100043d0000000002130049000000a0022000390000000003000019004e000a0000040f004e00140000040f0000004e000004320000004f0001042e000000500001043000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ffffffffffffffff000000000000000000000000000000000000000000000000000000008903573000000000000000000000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffff0000000000000000000000000000000000000000000000000000000000ffffff0000000000008000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80000000000000000000000000000000000000000000000000000000000000007fffff00000002000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();
        let compressed = compress_to_bytes(&example_code).unwrap();
        let decompressed = decompress_bytecode(&compressed);

        assert_eq!(example_code, decompressed);
    }

    #[test]
    fn bytecode_compression_statisticst() {
        let example_code =
            hex::decode("0000000000000000111111111111111111111111111111112222222222222222")
                .unwrap();
        // The size of the dictionary should be `0x0003`
        // The dictionary itself should put the most common chunk first, i.e. `0x1111111111111111`
        // Then, the ordering does not matter, but the algorithm will return the one with the highest position, i.e. `0x2222222222222222`
        let expected_encoding =
            hex::decode("00031111111111111111222222222222222200000000000000000002000000000001")
                .unwrap();

        assert_eq!(expected_encoding, compress_to_bytes(&example_code).unwrap());
    }
}
