use serde::{Deserialize, Serialize};

use crate::{bytecode::BytecodeMarker, web3::keccak256, H256};

/// An identifier of the contract bytecode.
/// This identifier can be used to detect different contracts that share the same sources,
/// even if they differ in bytecode verbatim (e.g. if the contract metadata is different).
///
/// Identifier depends on the marker of the bytecode of the contract.
/// This might be important, since the metadata can be different for EVM and EraVM,
/// e.g. `zksolc` [supports][zksolc_keccak] keccak256 hash of the metadata as an alternative to CBOR.
///
/// [zksolc_keccak]: https://matter-labs.github.io/era-compiler-solidity/latest/02-command-line-interface.html#--metadata-hash
// Note: there are missing opportunities here, e.g. Etherscan is able to detect the contracts
// that differ in creation bytecode and/or constructor arguments (for partial match). This is
// less relevant for ZKsync, since there is no concept of creation bytecode there; although
// this may become needed if we will extend the EVM support.
#[derive(Debug, Clone, Copy)]
pub struct ContractIdentifier {
    /// Marker of the bytecode of the contract.
    pub bytecode_marker: BytecodeMarker,
    /// keccak256 hash of the full contract bytecode.
    /// Can be used as an identifier of precise contract compilation.
    pub bytecode_keccak256: H256,
    /// keccak256 hash of the contract bytecode without metadata (e.g. with either
    /// CBOR or keccak256 metadata hash being stripped).
    /// If no metadata is detected, equal to `bytecode_keccak256`.
    pub bytecode_without_metadata_keccak256: H256,
    /// Kind of detected metadata.
    pub detected_metadata: Option<DetectedMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Match {
    /// Contracts are identical.
    Full,
    /// Metadata is different.
    Partial,
    /// No match.
    None,
}

/// Metadata detected in the contract bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedMetadata {
    /// keccak256 metadata (only for EraVM)
    Keccak256,
    /// CBOR metadata
    Cbor,
}

/// Possible values for the metadata hashes structure.
/// Details can be found here: https://docs.soliditylang.org/en/latest/metadata.html
///
/// We're not really interested in the values here, we just want to make sure that we
/// can deserialize the metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CborMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    ipfs: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bzzr1: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bzzr0: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    experimental: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    solc: Option<Vec<u8>>,
}

impl ContractIdentifier {
    pub fn from_bytecode(bytecode_marker: BytecodeMarker, bytecode: &[u8]) -> Self {
        // Calculate the hash for bytecode with metadata.
        let bytecode_keccak256 = H256(keccak256(bytecode));

        // Try to detect metadata.
        // CBOR takes precedence (since keccak doesn't have direct markers, so it's partially a
        // fallback).
        let (detected_metadata, bytecode_without_metadata_keccak256) =
            if let Some(hash) = Self::detect_cbor_metadata(bytecode_marker, bytecode) {
                (Some(DetectedMetadata::Cbor), hash)
            } else if let Some(hash) = Self::detect_keccak_metadata(bytecode_marker, bytecode) {
                (Some(DetectedMetadata::Keccak256), hash)
            } else {
                // Fallback
                (None, bytecode_keccak256)
            };

        Self {
            bytecode_marker,
            bytecode_keccak256,
            bytecode_without_metadata_keccak256,
            detected_metadata,
        }
    }

    /// Will try to detect keccak256 metadata hash (only for EraVM)
    fn detect_keccak_metadata(bytecode_marker: BytecodeMarker, bytecode: &[u8]) -> Option<H256> {
        // For EraVM, the one option for metadata hash is keccak256 hash of the metadata.
        if bytecode_marker == BytecodeMarker::EraVm {
            // For metadata, we might have padding: it takes either 32 or 64 bytes depending
            // on whether the amount of words in the contract is odd, so we need to check
            // if there is padding.
            let bytecode_without_metadata = Self::strip_padding(bytecode, 32)?;
            let hash = H256(keccak256(bytecode_without_metadata));
            Some(hash)
        } else {
            None
        }
    }

    /// Will try to detect CBOR metadata.
    fn detect_cbor_metadata(bytecode_marker: BytecodeMarker, bytecode: &[u8]) -> Option<H256> {
        let length = bytecode.len();

        // Last two bytes is the length of the metadata in big endian.
        if length < 2 {
            return None;
        }
        let metadata_length =
            u16::from_be_bytes([bytecode[length - 2], bytecode[length - 1]]) as usize;
        // Including size
        let full_metadata_length = metadata_length + 2;

        // Get slice for the metadata.
        if length < full_metadata_length {
            return None;
        }
        let raw_metadata = &bytecode[length - full_metadata_length..length - 2];
        // Try decoding. We are not interested in the actual value.
        let _metadata: CborMetadata = match ciborium::from_reader(raw_metadata) {
            Ok(metadata) => metadata,
            Err(_) => return None,
        };

        // Strip metadata and calculate hash.
        let bytecode_without_metadata = match bytecode_marker {
            BytecodeMarker::Evm => {
                // On EVM, there is no padding.
                &bytecode[..length - full_metadata_length]
            }
            BytecodeMarker::EraVm => {
                // On EraVM, there is padding:
                // 1. We must align the metadata length to 32 bytes.
                // 2. We may need to add 32 bytes of padding.
                let aligned_metadata_length = metadata_length.div_ceil(32) * 32;
                Self::strip_padding(bytecode, aligned_metadata_length)?
            }
        };
        let hash = H256(keccak256(bytecode_without_metadata));
        Some(hash)
    }

    /// Adds one word to the metadata length and check if it's a padding word.
    /// If it is, strips the padding.
    /// Returns `None` if `metadata_length` + padding won't fit into the bytecode.
    fn strip_padding(bytecode: &[u8], metadata_length: usize) -> Option<&[u8]> {
        const PADDING_WORD: [u8; 32] = [0u8; 32];

        let length = bytecode.len();
        let metadata_with_padding_length = metadata_length + 32;
        if length < metadata_with_padding_length {
            return None;
        }
        if bytecode[length - metadata_with_padding_length..length - metadata_length] == PADDING_WORD
        {
            // Padding was added, strip it.
            Some(&bytecode[..length - metadata_with_padding_length])
        } else {
            // Padding wasn't added, strip metadata only.
            Some(&bytecode[..length - metadata_length])
        }
    }

    /// Checks the kind of match between identifier and other bytecode.
    pub fn matches(&self, other: &[u8]) -> Match {
        let other_identifier = Self::from_bytecode(self.bytecode_marker, other);

        if self.bytecode_keccak256 == other_identifier.bytecode_keccak256 {
            return Match::Full;
        }

        // Check if metadata is different.
        // Note that here we do not handle "complex" cases, e.g. lack of metadata in one contract
        // and presence in another, or different kinds of metadata. This is OK: partial
        // match is needed mostly when you cannot reproduce the original metadata, but one always
        // can submit the contract with the same metadata kind.
        if self.bytecode_without_metadata_keccak256
            == other_identifier.bytecode_without_metadata_keccak256
        {
            return Match::Partial;
        }

        Match::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eravm_cbor_without_padding() {
        // Sample contract with no methods, compiled from the root of monorepo with:
        // ./etc/zksolc-bin/v1.5.8/zksolc --solc ./etc/solc-bin/zkVM-0.8.28-1.0.1/solc --metadata-hash ipfs --codegen yul test.sol --bin
        // (Use `zkstack contract-verifier init` to download compilers)
        let data = hex::decode("0000008003000039000000400030043f0000000100200190000000110000c13d0000000900100198000000190000613d000000000101043b0000000a011001970000000b0010009c000000190000c13d0000000001000416000000000001004b000000190000c13d000000000100041a000000800010043f0000000c010000410000001c0001042e0000000001000416000000000001004b000000190000c13d00000020010000390000010000100443000001200000044300000008010000410000001c0001042e00000000010000190000001d000104300000001b000004320000001c0001042e0000001d0001043000000000000000000000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000fffffffc000000000000000000000000ffffffff000000000000000000000000000000000000000000000000000000006d4ce63c0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000008000000000000000000000000000000000000000000000000000000000a16469706673582212208acf048570dcc1c3ff41bf8f20376049a42ae8a471f2b2ae8c14d8b356d86d79002a").unwrap();
        let bytecode_keccak256 = H256(keccak256(&data));
        let full_metadata_len = 64; // (CBOR metadata + len bytes)
        let bytecode_without_metadata_keccak256 =
            H256(keccak256(&data[..data.len() - full_metadata_len]));

        let identifier = ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, &data);
        assert_eq!(
            identifier.bytecode_keccak256, bytecode_keccak256,
            "Incorrect bytecode hash"
        );
        assert_eq!(
            identifier.detected_metadata,
            Some(DetectedMetadata::Cbor),
            "Incorrect detected metadata"
        );
        assert_eq!(
            identifier.bytecode_without_metadata_keccak256, bytecode_without_metadata_keccak256,
            "Incorrect bytecode without metadata hash"
        );
    }

    #[test]
    fn eravm_cbor_with_padding() {
        // Same as `eravm_cbor_without_padding` but now bytecode has padding.
        let data = hex::decode("00000001002001900000000c0000613d0000008001000039000000400010043f0000000001000416000000000001004b0000000c0000c13d00000020010000390000010000100443000001200000044300000005010000410000000f0001042e000000000100001900000010000104300000000e000004320000000f0001042e0000001000010430000000000000000000000000000000000000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a1646970667358221220d5be4da510b089bb58fa6c65f0a387eef966bcf48671a24fb2b1bc7190842978002a").unwrap();
        let bytecode_keccak256 = H256(keccak256(&data));
        let full_metadata_len = 64 + 32; // (CBOR metadata + len bytes + padding)
        let bytecode_without_metadata_keccak256 =
            H256(keccak256(&data[..data.len() - full_metadata_len]));

        let identifier = ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, &data);
        assert_eq!(
            identifier.bytecode_keccak256, bytecode_keccak256,
            "Incorrect bytecode hash"
        );
        assert_eq!(
            identifier.detected_metadata,
            Some(DetectedMetadata::Cbor),
            "Incorrect detected metadata"
        );
        assert_eq!(
            identifier.bytecode_without_metadata_keccak256, bytecode_without_metadata_keccak256,
            "Incorrect bytecode without metadata hash"
        );
    }

    #[test]
    fn eravm_keccak_without_padding() {
        // Sample contract with no methods, compiled from the root of monorepo with:
        // ./etc/zksolc-bin/v1.5.8/zksolc --solc ./etc/solc-bin/zkVM-0.8.28-1.0.1/solc --metadata-hash keccak256 --codegen yul test.sol --bin
        // (Use `zkstack contract-verifier init` to download compilers)
        let data = hex::decode("00000001002001900000000c0000613d0000008001000039000000400010043f0000000001000416000000000001004b0000000c0000c13d00000020010000390000010000100443000001200000044300000005010000410000000f0001042e000000000100001900000010000104300000000e000004320000000f0001042e000000100001043000000000000000000000000000000000000000000000000000000002000000000000000000000000000000400000010000000000000000000a00e4a5f19bb139176aa501024c7032404c065bc0012897fefd9ebc7e9a7677").unwrap();
        let bytecode_keccak256 = H256(keccak256(&data));
        let full_metadata_len = 32; // (keccak only)
        let bytecode_without_metadata_keccak256 =
            H256(keccak256(&data[..data.len() - full_metadata_len]));

        let identifier = ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, &data);
        assert_eq!(
            identifier.bytecode_keccak256, bytecode_keccak256,
            "Incorrect bytecode hash"
        );
        assert_eq!(
            identifier.detected_metadata,
            Some(DetectedMetadata::Keccak256),
            "Incorrect detected metadata"
        );
        assert_eq!(
            identifier.bytecode_without_metadata_keccak256, bytecode_without_metadata_keccak256,
            "Incorrect bytecode without metadata hash"
        );
    }

    #[test]
    fn eravm_keccak_with_padding() {
        // Same as `eravm_keccak_without_padding`, but now bytecode has padding.
        let data = hex::decode("0000008003000039000000400030043f0000000100200190000000110000c13d0000000900100198000000190000613d000000000101043b0000000a011001970000000b0010009c000000190000c13d0000000001000416000000000001004b000000190000c13d000000000100041a000000800010043f0000000c010000410000001c0001042e0000000001000416000000000001004b000000190000c13d00000020010000390000010000100443000001200000044300000008010000410000001c0001042e00000000010000190000001d000104300000001b000004320000001c0001042e0000001d0001043000000000000000000000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000fffffffc000000000000000000000000ffffffff000000000000000000000000000000000000000000000000000000006d4ce63c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002000000080000000000000000000000000000000000000000000000000000000000000000000000000000000009b1f0a6172ae84051eca37db231c0fa6249349f4ddaf86a87474a587c19d946d").unwrap();
        let bytecode_keccak256 = H256(keccak256(&data));
        let full_metadata_len = 64; // (keccak + padding)
        let bytecode_without_metadata_keccak256 =
            H256(keccak256(&data[..data.len() - full_metadata_len]));

        let identifier = ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, &data);
        assert_eq!(
            identifier.bytecode_keccak256, bytecode_keccak256,
            "Incorrect bytecode hash"
        );
        assert_eq!(
            identifier.detected_metadata,
            Some(DetectedMetadata::Keccak256),
            "Incorrect detected metadata"
        );
        assert_eq!(
            identifier.bytecode_without_metadata_keccak256, bytecode_without_metadata_keccak256,
            "Incorrect bytecode without metadata hash"
        );
    }

    #[test]
    fn eravm_too_short_bytecode() {
        // Random short bytecode
        let data = hex::decode("0000008003000039000000400030043f0000000100200190000000110000c13d")
            .unwrap();
        let bytecode_keccak256 = H256(keccak256(&data));

        let identifier = ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, &data);
        assert_eq!(
            identifier.bytecode_keccak256, bytecode_keccak256,
            "Incorrect bytecode hash"
        );
        assert_eq!(
            identifier.detected_metadata, None,
            "Incorrect detected metadata"
        );
        // When no metadata is detected, `bytecode_without_metadata_keccak256` is equal to
        // `bytecode_keccak256`.
        assert_eq!(
            identifier.bytecode_without_metadata_keccak256, bytecode_keccak256,
            "Incorrect bytecode without metadata hash"
        );
    }

    #[test]
    fn evm_none() {
        // Sample contract with no methods, compiled from the root of monorepo with:
        // ./etc/solc-bin/0.8.28/solc test.sol --bin --no-cbor-metadata
        // (Use `zkstack contract-verifier init` to download compilers)
        let data = hex::decode("6080604052348015600e575f5ffd5b50607980601a5f395ff3fe6080604052348015600e575f5ffd5b50600436106026575f3560e01c80636d4ce63c14602a575b5f5ffd5b60306044565b604051603b91906062565b60405180910390f35b5f5f54905090565b5f819050919050565b605c81604c565b82525050565b5f60208201905060735f8301846055565b9291505056").unwrap();
        let bytecode_keccak256 = H256(keccak256(&data));

        let identifier = ContractIdentifier::from_bytecode(BytecodeMarker::Evm, &data);
        assert_eq!(
            identifier.bytecode_keccak256, bytecode_keccak256,
            "Incorrect bytecode hash"
        );
        assert_eq!(
            identifier.detected_metadata, None,
            "Incorrect detected metadata"
        );
        // When no metadata is detected, `bytecode_without_metadata_keccak256` is equal to
        // `bytecode_keccak256`.
        assert_eq!(
            identifier.bytecode_without_metadata_keccak256, bytecode_keccak256,
            "Incorrect bytecode without metadata hash"
        );
    }

    #[test]
    fn evm_cbor() {
        // ./etc/solc-bin/0.8.28/solc test.sol --bin --metadata-hash ipfs
        let ipfs_bytecode = "6080604052348015600e575f5ffd5b5060af80601a5f395ff3fe6080604052348015600e575f5ffd5b50600436106026575f3560e01c80636d4ce63c14602a575b5f5ffd5b60306044565b604051603b91906062565b60405180910390f35b5f5f54905090565b5f819050919050565b605c81604c565b82525050565b5f60208201905060735f8301846055565b9291505056fea2646970667358221220bca846db362b62d2eb9891565b12433410e0f6a634657d2c7d1e7469447e8ab564736f6c634300081c0033";
        // ./etc/solc-bin/0.8.28/solc test.sol --bin --metadata-hash none
        // Note that cbor will still be included but will only have solc version.
        let none_bytecode = "6080604052348015600e575f5ffd5b50608680601a5f395ff3fe6080604052348015600e575f5ffd5b50600436106026575f3560e01c80636d4ce63c14602a575b5f5ffd5b60306044565b604051603b91906062565b60405180910390f35b5f5f54905090565b5f819050919050565b605c81604c565b82525050565b5f60208201905060735f8301846055565b9291505056fea164736f6c634300081c000a";
        // ./etc/solc-bin/0.8.28/solc test.sol --bin --metadata-hash swarm
        let swarm_bytecode = "6080604052348015600e575f5ffd5b5060ae80601a5f395ff3fe6080604052348015600e575f5ffd5b50600436106026575f3560e01c80636d4ce63c14602a575b5f5ffd5b60306044565b604051603b91906062565b60405180910390f35b5f5f54905090565b5f819050919050565b605c81604c565b82525050565b5f60208201905060735f8301846055565b9291505056fea265627a7a72315820c0def30c57166e97d6a58290213f3b0d1f83532e7a0371c8e2b6dba826bae46164736f6c634300081c0032";

        // Different variations of the same contract, compiled with different metadata options.
        // Tuples of (label, bytecode, size of metadata (including length)).
        // Size of metadata can be found using https://playground.sourcify.dev/
        let test_vector = [
            ("ipfs", ipfs_bytecode, 51usize + 2),
            ("none", none_bytecode, 10 + 2),
            ("swarm", swarm_bytecode, 50 + 2),
        ];

        for (label, bytecode, full_metadata_len) in test_vector {
            let data = hex::decode(bytecode).unwrap();
            let bytecode_keccak256 = H256(keccak256(&data));
            let bytecode_without_metadata_keccak256 =
                H256(keccak256(&data[..data.len() - full_metadata_len]));

            let identifier = ContractIdentifier::from_bytecode(BytecodeMarker::Evm, &data);
            assert_eq!(
                identifier.bytecode_keccak256, bytecode_keccak256,
                "{label}: Incorrect bytecode hash"
            );
            assert_eq!(
                identifier.detected_metadata,
                Some(DetectedMetadata::Cbor),
                "{label}: Incorrect detected metadata"
            );
            assert_eq!(
                identifier.bytecode_without_metadata_keccak256, bytecode_without_metadata_keccak256,
                "{label}: Incorrect bytecode without metadata hash"
            );
        }
    }
}
