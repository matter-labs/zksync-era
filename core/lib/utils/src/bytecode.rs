// FIXME: move to basic_types?

use zk_evm::k256::sha2::{Digest, Sha256};
use zksync_basic_types::H256;

use crate::bytes_to_chunks;

const MAX_BYTECODE_LENGTH_IN_WORDS: usize = (1 << 16) - 1;
const MAX_BYTECODE_LENGTH_BYTES: usize = MAX_BYTECODE_LENGTH_IN_WORDS * 32;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum InvalidBytecodeError {
    #[error("Bytecode too long: {0} bytes, while max {1} allowed")]
    BytecodeTooLong(usize, usize),
    #[error("Bytecode has even number of 32-byte words")]
    BytecodeLengthInWordsIsEven,
    #[error("Bytecode length is not divisible by 32")]
    BytecodeLengthIsNotDivisibleBy32,
}

pub fn validate_bytecode(code: &[u8]) -> Result<(), InvalidBytecodeError> {
    let bytecode_len = code.len();

    if bytecode_len > MAX_BYTECODE_LENGTH_BYTES {
        return Err(InvalidBytecodeError::BytecodeTooLong(
            bytecode_len,
            MAX_BYTECODE_LENGTH_BYTES,
        ));
    }

    if bytecode_len % 32 != 0 {
        return Err(InvalidBytecodeError::BytecodeLengthIsNotDivisibleBy32);
    }

    let bytecode_len_words = bytecode_len / 32;

    if bytecode_len_words % 2 == 0 {
        return Err(InvalidBytecodeError::BytecodeLengthInWordsIsEven);
    }

    Ok(())
}

/// Hashes the provided EraVM bytecode.
pub fn hash_bytecode(code: &[u8]) -> H256 {
    let chunked_code = bytes_to_chunks(code);
    let hash = zk_evm::zkevm_opcode_defs::utils::bytecode_to_code_hash(&chunked_code)
        .expect("Invalid bytecode");

    H256(hash)
}

pub fn bytecode_len_in_words(bytecodehash: &H256) -> u16 {
    u16::from_be_bytes([bytecodehash[2], bytecodehash[3]])
}

pub fn bytecode_len_in_bytes(bytecodehash: H256) -> usize {
    bytecode_len_in_words(&bytecodehash) as usize * 32
}

/// Bytecode marker encoded in the first byte of the bytecode hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BytecodeMarker {
    /// EraVM bytecode marker (1).
    EraVm = 1,
    /// EVM bytecode marker (2).
    Evm = 2,
}

impl BytecodeMarker {
    /// Parses a marker from the bytecode hash.
    pub fn new(bytecode_hash: H256) -> Option<Self> {
        Some(match bytecode_hash.as_bytes()[0] {
            val if val == Self::EraVm as u8 => Self::EraVm,
            val if val == Self::Evm as u8 => Self::Evm,
            _ => return None,
        })
    }
}

/// Hashes the provided EVM bytecode. The bytecode must be padded to an odd number of 32-byte words;
/// bytecodes stored in the known codes storage satisfy this requirement automatically.
pub fn hash_evm_bytecode(bytecode: &[u8]) -> H256 {
    validate_bytecode(bytecode).expect("invalid EVM bytecode");

    let mut hasher = Sha256::new();
    let len = bytecode.len() as u16;
    hasher.update(bytecode);
    let result = hasher.finalize();

    let mut output = [0u8; 32];
    output[..].copy_from_slice(result.as_slice());
    output[0] = BytecodeMarker::Evm as u8;
    output[1] = 0;
    output[2..4].copy_from_slice(&len.to_be_bytes());

    H256(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytecode_markers_are_valid() {
        let bytecode_hash = hash_bytecode(&[0; 32]);
        assert_eq!(
            BytecodeMarker::new(bytecode_hash),
            Some(BytecodeMarker::EraVm)
        );
        let bytecode_hash = hash_evm_bytecode(&[0; 32]);
        assert_eq!(
            BytecodeMarker::new(bytecode_hash),
            Some(BytecodeMarker::Evm)
        );
    }
}
