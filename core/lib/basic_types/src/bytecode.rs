//! Bytecode-related types and utils.
//!
//! # Bytecode kinds
//!
//! ZKsync supports 2 kinds of bytecodes: EraVM and EVM ones.
//!
//! - **EraVM** bytecodes consist of 64-bit (8-byte) instructions for the corresponding VM.
//! - **EVM** bytecodes consist of ordinary EVM opcodes, preceded with a 32-byte big-endian code length (in bytes).
//!
//! Both bytecode kinds are right-padded to consist of an integer, odd number of 32-byte words. All methods
//! in this module operate on padded bytecodes unless explicitly specified otherwise.

use std::iter;

use anyhow::Context as _;
use sha2::{Digest, Sha256};

use crate::{H256, U256};

const MAX_BYTECODE_LENGTH_IN_WORDS: usize = (1 << 16) - 1;
const MAX_BYTECODE_LENGTH_BYTES: usize = MAX_BYTECODE_LENGTH_IN_WORDS * 32;

/// Errors returned from [`validate_bytecode()`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidBytecodeError {
    /// Bytecode is too long.
    #[error("Bytecode too long: {0} bytes, while max {1} allowed")]
    BytecodeTooLong(usize, usize),
    /// Bytecode length isn't divisible by 32 (i.e., bytecode cannot be represented as a sequence of 32-byte EraVM words).
    #[error("Bytecode length is not divisible by 32")]
    BytecodeLengthIsNotDivisibleBy32,
    /// Bytecode has an even number of 32-byte words.
    #[error("Bytecode has even number of 32-byte words")]
    BytecodeLengthInWordsIsEven,
}

/// Validates that the given bytecode passes basic checks (e.g., not too long).
///
/// The performed checks are universal both for EraVM and (padded) EVM bytecodes. If you need to additionally check EVM bytecode integrity,
/// use [`trim_padded_evm_bytecode()`].
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

/// 32-byte bytecode hash. Besides a cryptographically secure hash of the bytecode contents, contains a [`BytecodeMarker`]
/// and the bytecode length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BytecodeHash(H256);

impl BytecodeHash {
    /// Hashes the provided EraVM bytecode.
    pub fn for_bytecode(bytecode: &[u8]) -> Self {
        Self::for_generic_bytecode(BytecodeMarker::EraVm, bytecode, bytecode.len())
    }

    /// Hashes the provided padded EVM bytecode.
    pub fn for_evm_bytecode(raw_bytecode_len: usize, bytecode: &[u8]) -> Self {
        Self::for_generic_bytecode(BytecodeMarker::Evm, bytecode, raw_bytecode_len)
    }

    /// Hashes the provided raw EVM bytecode.
    pub fn for_raw_evm_bytecode(bytecode: &[u8]) -> Self {
        let padded_evm_bytecode = pad_evm_bytecode(bytecode);
        Self::for_evm_bytecode(bytecode.len(), &padded_evm_bytecode)
    }

    fn for_generic_bytecode(
        kind: BytecodeMarker,
        bytecode: &[u8],
        bytecode_len_in_bytes: usize,
    ) -> Self {
        validate_bytecode(bytecode).expect("invalid bytecode");

        let mut hasher = Sha256::new();
        let len = match kind {
            BytecodeMarker::EraVm => (bytecode_len_in_bytes / 32) as u16,
            BytecodeMarker::Evm => bytecode_len_in_bytes as u16,
        };
        hasher.update(bytecode);
        let result = hasher.finalize();

        let mut output = [0u8; 32];
        output[..].copy_from_slice(result.as_slice());
        output[0] = kind as u8;
        output[1] = 0;
        output[2..4].copy_from_slice(&len.to_be_bytes());

        Self(H256(output))
    }

    /// Returns a marker / kind of this bytecode.
    pub fn marker(&self) -> BytecodeMarker {
        match self.0.as_bytes()[0] {
            val if val == BytecodeMarker::EraVm as u8 => BytecodeMarker::EraVm,
            val if val == BytecodeMarker::Evm as u8 => BytecodeMarker::Evm,
            _ => unreachable!(),
        }
    }

    /// Returns the length of the hashed bytecode in bytes.
    pub fn len_in_bytes(&self) -> usize {
        let bytes = self.0.as_bytes();
        let raw_len = u16::from_be_bytes([bytes[2], bytes[3]]);
        match self.marker() {
            BytecodeMarker::EraVm => raw_len as usize * 32,
            BytecodeMarker::Evm => raw_len as usize,
        }
    }

    /// Returns the underlying hash value.
    pub fn value(self) -> H256 {
        self.0
    }

    /// Returns the underlying hash value interpreted as a big-endian unsigned integer.
    pub fn value_u256(self) -> U256 {
        crate::h256_to_u256(self.0)
    }
}

impl TryFrom<H256> for BytecodeHash {
    type Error = anyhow::Error;

    fn try_from(raw_hash: H256) -> Result<Self, Self::Error> {
        BytecodeMarker::new(raw_hash).context("unknown bytecode hash marker")?;
        Ok(Self(raw_hash))
    }
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

    /// Performs the best guess as to which kind of bytecode the provided `raw_bytecode` is.
    pub fn detect(raw_bytecode: &[u8]) -> Self {
        if validate_bytecode(raw_bytecode).is_err() {
            // Certainly not an EraVM bytecode
            Self::Evm
        } else if raw_bytecode.first() == Some(&0) {
            // The vast majority of EraVM bytecodes observed in practice start with the 0 byte.
            // (This is the upper byte of the second immediate in the first instruction.)
            // OTOH, EVM bytecodes don't usually start with the 0 byte (i.e., STOP opcode). In particular,
            // using such a bytecode in state overrides is useless.
            Self::EraVm
        } else {
            Self::Evm
        }
    }
}

/// Removes padding from the bytecode, if necessary.
pub fn trim_bytecode(bytecode_hash: BytecodeHash, raw: &[u8]) -> anyhow::Result<&[u8]> {
    match bytecode_hash.marker() {
        BytecodeMarker::EraVm => Ok(raw),
        BytecodeMarker::Evm => trim_padded_evm_bytecode(bytecode_hash, raw),
    }
}

/// Removes padding from an EVM bytecode, returning the original EVM bytecode.
pub fn trim_padded_evm_bytecode(bytecode_hash: BytecodeHash, raw: &[u8]) -> anyhow::Result<&[u8]> {
    if bytecode_hash.marker() != BytecodeMarker::Evm {
        anyhow::bail!("only EVM bytecode hashes allowed")
    }
    validate_bytecode(raw).context("bytecode fails basic validity checks")?;

    // Actual raw unpadded EVM bytecode length is encoded in bytecode hash
    let bytecode_len: usize = bytecode_hash.len_in_bytes();
    let bytecode = raw.get(0..bytecode_len).with_context(|| {
        format!(
            "encoded length ({bytecode_len}) exceeds real length ({})",
            raw.len()
        )
    })?;
    // Since slicing above succeeded, this one is safe.
    let padding = &raw[bytecode_len..];
    anyhow::ensure!(
        padding.iter().all(|&b| b == 0),
        "bytecode padding contains non-zero bytes"
    );
    Ok(bytecode)
}

/// Pads an EVM bytecode in the same ways it's done by system contracts.
pub fn pad_evm_bytecode(deployed_bytecode: &[u8]) -> Vec<u8> {
    let mut padded = Vec::with_capacity(deployed_bytecode.len());
    padded.extend_from_slice(deployed_bytecode);

    // Pad to the 32-byte word boundary.
    if padded.len() % 32 != 0 {
        padded.extend(iter::repeat_n(0, 32 - padded.len() % 32));
    }
    assert_eq!(padded.len() % 32, 0);

    // Pad to contain the odd number of words.
    if (padded.len() / 32) % 2 != 1 {
        padded.extend_from_slice(&[0; 32]);
    }
    assert_eq!((padded.len() / 32) % 2, 1);
    padded
}

#[doc(hidden)] // only useful for tests
pub mod testonly {
    use const_decoder::Decoder;

    pub const PADDED_EVM_BYTECODE: &[u8] = &const_decoder::decode!(
        Decoder::Hex,
        b"6080604052348015600e575f80fd5b50600436106030575f3560e01c8063816898ff146034578063\
          fb5343f314604c575b5f80fd5b604a60048036038101906046919060a6565b6066565b005b605260\
          6f565b604051605d919060d9565b60405180910390f35b805f8190555050565b5f5481565b5f80fd\
          5b5f819050919050565b6088816078565b81146091575f80fd5b50565b5f8135905060a081608156\
          5b92915050565b5f6020828403121560b85760b76074565b5b5f60c3848285016094565b91505092\
          915050565b60d3816078565b82525050565b5f60208201905060ea5f83018460cc565b9291505056\
          fea2646970667358221220caca1247066da378f2ec77c310f2ae51576272367b4fa11cc4350af4e9\
          ce4d0964736f6c634300081a00330000000000000000000000000000000000000000000000000000\
          0000000000000000000000000000000000000000000000000000000000000000"
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
        testonly::{PADDED_EVM_BYTECODE, PROCESSED_EVM_BYTECODE},
        *,
    };

    #[test]
    fn bytecode_markers_are_valid() {
        let bytecode_hash = BytecodeHash::for_bytecode(&[0; 32]);
        assert_eq!(bytecode_hash.marker(), BytecodeMarker::EraVm);
        assert_eq!(bytecode_hash.len_in_bytes(), 32);

        let bytecode_hash = BytecodeHash::for_raw_evm_bytecode(&[0; 32]);
        assert_eq!(bytecode_hash.marker(), BytecodeMarker::Evm);
        assert_eq!(bytecode_hash.len_in_bytes(), 32);

        let bytecode_hash = BytecodeHash::for_evm_bytecode(32, &[0; 96]);
        assert_eq!(bytecode_hash.marker(), BytecodeMarker::Evm);
        assert_eq!(bytecode_hash.len_in_bytes(), 32);
    }

    #[test]
    fn preparing_evm_bytecode() {
        let bytecode_hash =
            BytecodeHash::for_evm_bytecode(PROCESSED_EVM_BYTECODE.len(), PADDED_EVM_BYTECODE);
        let prepared = trim_padded_evm_bytecode(bytecode_hash, PADDED_EVM_BYTECODE).unwrap();
        assert_eq!(prepared, PROCESSED_EVM_BYTECODE);
    }
}
