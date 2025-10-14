//! A collection of functions that get reused throughout format versions.
use zksync_basic_types::{H160, H256, U256};

use super::{L2ToL1Pubdata, ParseError};
use crate::{
    l1_fetcher::constants::zksync::{
        L2_TO_L1_LOG_SERIALIZE_SIZE, LENGTH_BITS_OFFSET, OPERATION_BITMASK,
    },
    storage::PackingType,
};

pub struct ExtractedToken {
    pub l1_batch_number: U256,
    pub timestamp: U256,
    pub new_enumeration_index: U256,
    pub state_root: Vec<u8>,
    pub number_of_l1_txs: U256,
    pub priority_operations_hash: Vec<u8>,
    pub system_logs: Vec<u8>,
    pub total_l2_to_l1_pubdata: Vec<u8>,
}

// TODO: Maybe consider using an enum for post-boojum formats.
// I.e. V2::OnlyCalldata & V2::CalldataAndBlob.
impl TryFrom<&ethabi::Token> for ExtractedToken {
    type Error = ParseError;

    fn try_from(token: &ethabi::Token) -> Result<Self, Self::Error> {
        let ethabi::Token::Tuple(block_elems) = token else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "struct elements".to_string(),
            ));
        };

        let ethabi::Token::Uint(l1_batch_number) = block_elems[0].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "blockNumber".to_string(),
            ));
        };

        let ethabi::Token::Uint(timestamp) = block_elems[1].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo("timestamp".to_string()));
        };

        let ethabi::Token::Uint(new_enumeration_index) = block_elems[2].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "indexRepeatedStorageChanges".to_string(),
            ));
        };

        let ethabi::Token::FixedBytes(state_root) = block_elems[3].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "newStateRoot".to_string(),
            ));
        };

        let ethabi::Token::Uint(number_of_l1_txs) = block_elems[4].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "numberOfLayer1Txs".to_string(),
            ));
        };

        let ethabi::Token::FixedBytes(priority_operations_hash) = block_elems[5].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "priorityOperationsHash".to_string(),
            ));
        };

        let ethabi::Token::Bytes(system_logs) = block_elems[8].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo("systemLogs".to_string()));
        };

        let ethabi::Token::Bytes(total_l2_to_l1_pubdata) = block_elems[9].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "totalL2ToL1Pubdata".to_string(),
            ));
        };

        Ok(Self {
            l1_batch_number,
            timestamp,
            new_enumeration_index,
            state_root,
            number_of_l1_txs,
            priority_operations_hash,
            system_logs,
            total_l2_to_l1_pubdata,
        })
    }
}

// TODO: Move these to a dedicated parser struct.
pub fn parse_resolved_pubdata(bytes: &[u8]) -> Result<Vec<L2ToL1Pubdata>, ParseError> {
    let mut l2_to_l1_pubdata = Vec::new();

    let mut pointer = 0;
    // Skip over logs and messages.
    let num_of_l1_to_l2_logs = u32::from_be_bytes(read_next_n_bytes(bytes, &mut pointer));
    //pointer += L2_TO_L1_LOG_SERIALIZE_SIZE * num_of_l1_to_l2_logs as usize;
    for _ in 0..num_of_l1_to_l2_logs {
        l2_to_l1_pubdata.push(parse_l2_to_l1_log(bytes, &mut pointer)?);
    }
    //tracing::info!("num_of_l2_to_l1: {}", num_of_l1_to_l2_logs);

    let num_of_messages = u32::from_be_bytes(read_next_n_bytes(bytes, &mut pointer));
    for _ in 0..num_of_messages {
        l2_to_l1_pubdata.push(parse_l2_to_l1_message(bytes, &mut pointer)?);
    }

    // Parse published bytecodes.
    let num_of_bytecodes = u32::from_be_bytes(read_next_n_bytes(bytes, &mut pointer));
    //tracing::info!("num_of_bytecodes: {}", num_of_bytecodes);
    for _ in 0..num_of_bytecodes {
        let current_bytecode_len =
            u32::from_be_bytes(read_next_n_bytes(bytes, &mut pointer)) as usize;
        let bytecode = bytes[pointer..pointer + current_bytecode_len].to_vec();
        pointer += current_bytecode_len;
        l2_to_l1_pubdata.push(L2ToL1Pubdata::PublishedBytecode(bytecode))
    }

    let mut state_diffs = parse_compressed_state_diffs(bytes, &mut pointer)?;
    //tracing::info!("num_of_state_diffs: {}", state_diffs.len());
    l2_to_l1_pubdata.append(&mut state_diffs);

    Ok(l2_to_l1_pubdata)
}

pub fn parse_l2_to_l1_log(bytes: &[u8], pointer: &mut usize) -> Result<L2ToL1Pubdata, ParseError> {
    let start_pointer = *pointer;
    let result = L2ToL1Pubdata::L2ToL1Log {
        shard_id: u8::from_be_bytes(read_next_n_bytes(bytes, pointer)),
        is_service: u8::from_be_bytes(read_next_n_bytes(bytes, pointer)) != 0,
        tx_number_in_block: u16::from_be_bytes(read_next_n_bytes(bytes, pointer)),
        sender: H160::from_slice(&read_next_n_bytes::<20>(bytes, pointer)),
        key: H256::from_slice(&read_next_n_bytes::<32>(bytes, pointer)),
        value: H256::from_slice(&read_next_n_bytes::<32>(bytes, pointer)),
    };
    assert_eq!(L2_TO_L1_LOG_SERIALIZE_SIZE, *pointer - start_pointer);
    Ok(result)
}

pub fn parse_l2_to_l1_message(
    bytes: &[u8],
    pointer: &mut usize,
) -> Result<L2ToL1Pubdata, ParseError> {
    let current_message_len = u32::from_be_bytes(read_next_n_bytes(bytes, pointer));
    let result = L2ToL1Pubdata::L2ToL2Message(
        bytes[*pointer..*pointer + current_message_len as usize].to_vec(),
    );
    *pointer += current_message_len as usize;
    Ok(result)
}

pub fn parse_compressed_state_diffs(
    bytes: &[u8],
    pointer: &mut usize,
) -> Result<Vec<L2ToL1Pubdata>, ParseError> {
    let mut state_diffs = Vec::new();
    // Parse the header.
    let version = u8::from_be_bytes(read_next_n_bytes(bytes, pointer));
    if version != 1 {
        tracing::info!("Version: {}", version);
        return Err(ParseError::InvalidCompressedValue(String::from(
            "VersionMismatch",
        )));
    }

    if *pointer >= bytes.len() {
        return Ok(state_diffs);
    }

    let mut buffer = [0; 4];
    buffer[1..].copy_from_slice(&bytes[*pointer..*pointer + 3]);
    *pointer += 3;
    let _total_compressed_len = u32::from_be_bytes(buffer);

    let enumeration_index = u8::from_be_bytes(read_next_n_bytes(bytes, pointer));

    // Parse initial writes.
    let num_of_initial_writes = u16::from_be_bytes(read_next_n_bytes(bytes, pointer));
    for _ in 0..num_of_initial_writes {
        let derived_key = U256::from_little_endian(&read_next_n_bytes::<32>(bytes, pointer));

        let packing_type = read_compressed_value(bytes, pointer)?;
        state_diffs.push(L2ToL1Pubdata::CompressedStateDiff {
            is_repeated_write: false,
            derived_key,
            packing_type,
        });
    }

    // Parse repeated writes.
    while *pointer < bytes.len() {
        let derived_key = match enumeration_index {
            4 => U256::from_big_endian(&read_next_n_bytes::<4>(bytes, pointer)),
            5 => U256::from_big_endian(&read_next_n_bytes::<5>(bytes, pointer)),
            _ => {
                return Err(ParseError::InvalidCompressedValue(String::from(
                    "RepeatedDerivedKey",
                )))
            }
        };

        let packing_type = read_compressed_value(bytes, pointer)?;
        state_diffs.push(L2ToL1Pubdata::CompressedStateDiff {
            is_repeated_write: true,
            derived_key,
            packing_type,
        });
    }

    Ok(state_diffs)
}

pub fn read_compressed_value(bytes: &[u8], pointer: &mut usize) -> Result<PackingType, ParseError> {
    let metadata = u8::from_be_bytes(read_next_n_bytes(bytes, pointer));
    let operation = metadata & OPERATION_BITMASK;
    let len = if operation == 0 {
        32
    } else {
        metadata >> LENGTH_BITS_OFFSET
    } as usize;

    // Read compressed value.
    let mut buffer = [0; 32];
    let start = buffer.len() - len;
    let mut src_end = *pointer + len;
    let beyond = if src_end > bytes.len() {
        // can happen for v3 format, when we've chopped off too many zeros
        src_end - bytes.len()
    } else {
        0
    };
    src_end -= beyond;
    let dest_end = buffer.len() - beyond;
    buffer[start..dest_end].copy_from_slice(&bytes[*pointer..src_end]);
    *pointer += len;
    let compressed_value = U256::from_big_endian(&buffer);

    let packing_type = match operation {
        0 => PackingType::NoCompression(compressed_value),
        1 => PackingType::Add(compressed_value),
        2 => PackingType::Sub(compressed_value),
        3 => PackingType::Transform(compressed_value),
        _ => {
            return Err(ParseError::InvalidCompressedValue(String::from(
                "UnknownPackingType",
            )))
        }
    };

    Ok(packing_type)
}

pub fn read_next_n_bytes<const N: usize>(bytes: &[u8], pointer: &mut usize) -> [u8; N] {
    if *pointer >= bytes.len() {
        return [0; N];
    }
    let mut buffer = [0; N];
    buffer.copy_from_slice(&bytes[*pointer..*pointer + N]);
    *pointer += N;
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_compressed_value_common() {
        let buf = [
            0, 1, 0, 0, 197, 168, 90, 55, 47, 68, 26, 198, 147, 33, 10, 24, 230, 131, 181, 48, 190,
            216, 117, 253, 202, 178, 247, 225, 1, 176, 87, 212, 51,
        ];
        let mut pointer = 0;
        let packing_type = read_compressed_value(&buf, &mut pointer).unwrap();
        let PackingType::NoCompression(_n) = packing_type else {
            panic!("packing_type = {:?}", packing_type);
        };
    }

    #[test]
    fn parse_compressed_value_add() {
        let buf = [9, 1];
        let mut pointer = 0;
        let packing_type = read_compressed_value(&buf, &mut pointer).unwrap();
        if let PackingType::Add(n) = packing_type {
            assert_eq!(n, U256::one());
        } else {
            panic!("packing_type = {:?}", packing_type);
        }
    }
}
