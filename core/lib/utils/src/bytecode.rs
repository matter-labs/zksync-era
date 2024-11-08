// FIXME: move to basic_types?

use anyhow::Context as _;
use zk_evm::k256::sha2::{Digest, Sha256};
use zksync_basic_types::{H256, U256};

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

pub fn prepare_evm_bytecode(raw: &[u8]) -> anyhow::Result<&[u8]> {
    // EVM bytecodes are prefixed with a big-endian `U256` bytecode length.
    let bytecode_len_bytes = raw.get(..32).context("length < 32")?;
    let bytecode_len = U256::from_big_endian(bytecode_len_bytes);
    let bytecode_len: usize = bytecode_len
        .try_into()
        .map_err(|_| anyhow::anyhow!("length ({bytecode_len}) overflow"))?;
    let bytecode = raw.get(32..(32 + bytecode_len)).with_context(|| {
        format!(
            "prefixed length ({bytecode_len}) exceeds real length ({})",
            raw.len() - 32
        )
    })?;
    // Since slicing above succeeded, this one is safe.
    let padding = &raw[(32 + bytecode_len)..];
    anyhow::ensure!(
        padding.iter().all(|&b| b == 0),
        "bytecode padding contains non-zero bytes"
    );
    Ok(bytecode)
}

pub mod testonly {
    use const_decoder::Decoder;

    pub const RAW_EVM_BYTECODE: &[u8] = &const_decoder::decode!(
        Decoder::Hex,
        b"00000000000000000000000000000000000000000000000000000000000001266080604052348015\
          600e575f80fd5b50600436106030575f3560e01c8063816898ff146034578063fb5343f314604c57\
          5b5f80fd5b604a60048036038101906046919060a6565b6066565b005b6052606f565b604051605d\
          919060d9565b60405180910390f35b805f8190555050565b5f5481565b5f80fd5b5f819050919050\
          565b6088816078565b81146091575f80fd5b50565b5f8135905060a0816081565b92915050565b5f\
          6020828403121560b85760b76074565b5b5f60c3848285016094565b91505092915050565b60d381\
          6078565b82525050565b5f60208201905060ea5f83018460cc565b9291505056fea2646970667358\
          221220caca1247066da378f2ec77c310f2ae51576272367b4fa11cc4350af4e9ce4d0964736f6c63\
          4300081a00330000000000000000000000000000000000000000000000000000"
    );
    pub const PROCESSED_EVM_BYTECODE: &[u8] = &const_decoder::decode!(
        Decoder::Hex,
        b"6080604052348015600e575f80fd5b50600436106030575f3560e01c8063816898ff146034578063\
          fb5343f314604c575b5f80fd5b604a60048036038101906046919060a6565b6066565b005b605260\
          6f565b604051605d919060d9565b60405180910390f35b805f8190555050565b5f5481565b5f80fd\
          5b5f819050919050565b6088816078565b81146091575f80fd5b50565b5f8135905060a081608156\
          5b92915050565b5f6020828403121560b85760b76074565b5b5f60c3848285016094565b91505092\
          915050565b60d3816078565b82525050565b5f60208201905060ea5f83018460cc565b9291505056\
          fea2646970667358221220caca1247066da378f2ec77c310f2ae51576272367b4fa11cc4350af4e9\
          ce4d0964736f6c634300081a0033"
    );
}

#[cfg(test)]
mod tests {
    use super::{
        testonly::{PROCESSED_EVM_BYTECODE, RAW_EVM_BYTECODE},
        *,
    };

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

    #[test]
    fn preparing_evm_bytecode() {
        let prepared = prepare_evm_bytecode(RAW_EVM_BYTECODE).unwrap();
        assert_eq!(prepared, PROCESSED_EVM_BYTECODE);
    }
}
