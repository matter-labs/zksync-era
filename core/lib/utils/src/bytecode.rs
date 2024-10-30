// FIXME: move to basic_types?

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
