use std::fmt;

use serde::{
    de::{self, Visitor},
    Deserialize, Serialize,
};

use crate::{bytecode::BytecodeMarker, web3::keccak256, H256};

/// An identifier of the contract bytecode.
///
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedMetadata {
    /// keccak256 metadata (only for EraVM)
    Keccak256,
    /// CBOR metadata
    Cbor {
        /// Length of metadata in the bytecode, including encoded length of CBOR and padding.
        full_length: usize,
        metadata: CborMetadata,
    },
}

impl DetectedMetadata {
    /// Returns full length (in bytes) of metadata in the bytecode.
    pub fn length(self) -> usize {
        match self {
            DetectedMetadata::Keccak256 => 32,
            DetectedMetadata::Cbor {
                full_length,
                metadata: _,
            } => full_length,
        }
    }
}

/// Represents the compiler version in the Cbor metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum CborCompilerVersion {
    /// For native solidity and vyper compilers, it is a 3 byte encoding of the compiler version: one byte each for major,
    /// minor and patch version number. For example, 0.8.28 is encoded as [0, 8, 28].
    /// More details can be found here:
    /// https://docs.soliditylang.org/en/latest/metadata.html#encoding-of-the-metadata-hash-in-the-bytecode
    Native(Vec<u8>),
    /// For ZKsync solidity compiler, the value consists of semicolon-separated pairs of colon-separated
    /// compiler names and versions. For example: "zksolc:<version>" or "zksolc:<version>;solc:<version>;llvm:<version>".
    /// More details can be found here:
    /// https://matter-labs.github.io/era-compiler-solidity/latest/02-command-line-interface.html#--metadata-hash
    ///
    /// For ZKsync vyper compiler, it's "zkvyper:<version>" or "zkvyper:<version>;vyper:<version>".
    /// More details can be found here:
    /// https://matter-labs.github.io/era-compiler-vyper/latest/02-command-line-interface.html#--metadata-hash
    ZKsync(String),
}

impl CborCompilerVersion {
    /// Returns the compiler versions from the metadata in a tuple (compiler_version, zk_compiler_version).
    pub fn get_compiler_versions(&self) -> (Option<String>, Option<String>) {
        match self {
            CborCompilerVersion::Native(compiler_version) => {
                // For native Solc and Vyper compilers, CBOR is a 3 byte encoding of the compiler version: one byte each
                // for major, minor and patch version number. For example, 0.8.28 is encoded as [0, 8, 28].
                if compiler_version.len() == 3 {
                    let version_str = format!(
                        "{}.{}.{}",
                        compiler_version[0], compiler_version[1], compiler_version[2]
                    );
                    (Some(version_str), None)
                } else {
                    (None, None)
                }
            }
            CborCompilerVersion::ZKsync(compiler_versions) => {
                // For ZKsync compilers, the value consists of semicolon-separated pairs of colon-separated compiler names
                // and versions. For example: "zksolc:1.5.13", "zkvyper:1.5.10;vyper:0.4.1" or
                // "zksolc:1.5.13;solc:0.8.29;llvm:1.0.2".
                let compilers_parts: Vec<&str> = compiler_versions
                    .split(';')
                    .filter_map(|part| part.split_once(':').map(|(_, value)| value))
                    .collect();

                if compilers_parts.is_empty() {
                    return (None, None);
                }
                let mut compiler_version = None;
                // Extract zk compiler version
                let zk_compiler_version = Some(format!("v{}", compilers_parts[0]));

                // Processing "zkvyper:<version>;vyper:<version>" version
                if compilers_parts.len() == 2 {
                    compiler_version = Some(compilers_parts[1].to_string());
                } else if compilers_parts.len() == 3 {
                    // Processing "zksolc:<version>;solc:<version>;llvm:<version>" version.
                    compiler_version = Some(format!(
                        "zkVM-{}-{}",
                        compilers_parts[1], compilers_parts[2]
                    ));
                }

                (compiler_version, zk_compiler_version)
            }
        }
    }
}

/// Possible values for the metadata hashes structure.
/// Details can be found here: https://docs.soliditylang.org/en/latest/metadata.html
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CborMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipfs: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bzzr1: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bzzr0: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<bool>,
    // CborMetadata is deserialized with ciborium which doesn't properly deserialize CborCompilerVersion
    // with it's variants. That's why we need a custom deserializer.
    #[serde(default, deserialize_with = "deserialize_cbor_compiler")]
    pub solc: Option<CborCompilerVersion>,
    #[serde(default, deserialize_with = "deserialize_cbor_compiler")]
    pub vyper: Option<CborCompilerVersion>,
}

impl CborMetadata {
    /// Returns the compiler versions from the metadata in a tuple (compiler_version, zk_compiler_version)
    /// for both solc and vyper.
    pub fn get_compiler_versions(&self) -> (Option<String>, Option<String>) {
        let compiler_version = self.solc.as_ref().or(self.vyper.as_ref());
        match compiler_version {
            Some(compiler_version) => compiler_version.get_compiler_versions(),
            None => (None, None),
        }
    }
}

struct CborCompilerVersionVisitor;
impl<'de> Visitor<'de> for CborCompilerVersionVisitor {
    type Value = Option<CborCompilerVersion>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a byte array or a string")
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Some(CborCompilerVersion::Native(value.to_vec())))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Some(CborCompilerVersion::ZKsync(value.to_string())))
    }
}

/// Custom deserializer for CborCompilerVersion so it's properly deserialized with ciborium.
fn deserialize_cbor_compiler<'de, D>(
    deserializer: D,
) -> Result<Option<CborCompilerVersion>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_any(CborCompilerVersionVisitor)
}

impl ContractIdentifier {
    pub fn from_bytecode(bytecode_marker: BytecodeMarker, bytecode: &[u8]) -> Self {
        // Calculate the hash for bytecode with metadata.
        let bytecode_keccak256 = H256(keccak256(bytecode));

        // Try to detect metadata.
        // CBOR takes precedence (since keccak doesn't have direct markers, so it's partially a
        // fallback).
        let (detected_metadata, bytecode_without_metadata_keccak256) =
            if let Some((full_length, hash, metadata)) =
                Self::detect_cbor_metadata(bytecode_marker, bytecode)
            {
                let detected_metadata = DetectedMetadata::Cbor {
                    full_length,
                    metadata,
                };
                (Some(detected_metadata), hash)
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
    fn detect_cbor_metadata(
        bytecode_marker: BytecodeMarker,
        bytecode: &[u8],
    ) -> Option<(usize, H256, CborMetadata)> {
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
        let metadata: CborMetadata = match ciborium::from_reader(raw_metadata) {
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
        Some((full_metadata_length, hash, metadata))
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
    pub fn matches(&self, other: &Self) -> Match {
        if self.bytecode_keccak256 == other.bytecode_keccak256 {
            return Match::Full;
        }

        // Check if metadata is different.
        // Note that here we do not handle "complex" cases, e.g. lack of metadata in one contract
        // and presence in another, or different kinds of metadata. This is OK: partial
        // match is needed mostly when you cannot reproduce the original metadata, but one always
        // can submit the contract with the same metadata kind.
        if self.bytecode_without_metadata_keccak256 == other.bytecode_without_metadata_keccak256 {
            return Match::Partial;
        }

        Match::None
    }

    /// Returns the length of the metadata in the bytecode.
    pub fn metadata_length(&self) -> usize {
        self.detected_metadata
            .clone()
            .map_or(0, DetectedMetadata::length)
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
            Some(DetectedMetadata::Cbor {
                full_length: 44,
                metadata: CborMetadata {
                    ipfs: Some(vec![
                        18, 32, 138, 207, 4, 133, 112, 220, 193, 195, 255, 65, 191, 143, 32, 55,
                        96, 73, 164, 42, 232, 164, 113, 242, 178, 174, 140, 20, 216, 179, 86, 216,
                        109, 121
                    ]),
                    bzzr1: None,
                    bzzr0: None,
                    experimental: None,
                    solc: None,
                    vyper: None
                }
            }),
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
            Some(DetectedMetadata::Cbor {
                full_length: 44,
                metadata: CborMetadata {
                    ipfs: Some(vec![
                        18, 32, 213, 190, 77, 165, 16, 176, 137, 187, 88, 250, 108, 101, 240, 163,
                        135, 238, 249, 102, 188, 244, 134, 113, 162, 79, 178, 177, 188, 113, 144,
                        132, 41, 120
                    ]),
                    bzzr1: None,
                    bzzr0: None,
                    experimental: None,
                    solc: None,
                    vyper: None
                }
            }),
            "Incorrect detected metadata"
        );
        assert_eq!(
            identifier.bytecode_without_metadata_keccak256, bytecode_without_metadata_keccak256,
            "Incorrect bytecode without metadata hash"
        );
    }

    #[test]
    fn eravm_cbor_with_solc() {
        // Sample contract with no methods, compiled from the root of monorepo with:
        // ./etc/zksolc-bin/v1.5.13/zksolc --solc ./etc/solc-bin/zkVM-0.8.24-1.0.1/solc --metadata-hash ipfs --codegen yul test.sol --bin
        // (Use `zkstack contract-verifier init` to download compilers)
        let data = hex::decode("00000001002001900000000c0000613d0000008001000039000000400010043f0000000001000416000000000001004b0000000c0000c13d00000020010000390000010000100443000001200000044300000005010000410000000f0001042e000000000100001900000010000104300000000e000004320000000f0001042e00000010000104300000000000000000000000000000000000000000000000000000000200000000000000000000000000000040000001000000000000000000000000000000000000a2646970667358221220322e344eb39f1d3acd2828278227de91da931750c529a47087ef31c8f3510cbb64736f6c6378247a6b736f6c633a312e352e31333b736f6c633a302e382e32343b6c6c766d3a312e302e310055").unwrap();
        let bytecode_keccak256 = H256(keccak256(&data));
        let full_metadata_len = 96; // (CBOR metadata + len bytes)
        let bytecode_without_metadata_keccak256 =
            H256(keccak256(&data[..data.len() - full_metadata_len]));

        let identifier: ContractIdentifier =
            ContractIdentifier::from_bytecode(BytecodeMarker::EraVm, &data);
        assert_eq!(
            identifier.bytecode_keccak256, bytecode_keccak256,
            "Incorrect bytecode hash"
        );
        assert_eq!(
            identifier.detected_metadata,
            Some(DetectedMetadata::Cbor {
                full_length: 87,
                metadata: CborMetadata {
                    ipfs: Some(vec![
                        18, 32, 50, 46, 52, 78, 179, 159, 29, 58, 205, 40, 40, 39, 130, 39, 222,
                        145, 218, 147, 23, 80, 197, 41, 164, 112, 135, 239, 49, 200, 243, 81, 12,
                        187
                    ]),
                    bzzr1: None,
                    bzzr0: None,
                    experimental: None,
                    solc: Some(CborCompilerVersion::ZKsync(
                        "zksolc:1.5.13;solc:0.8.24;llvm:1.0.1".to_string()
                    )),
                    vyper: None
                }
            }),
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
        // Tuples of (label, bytecode, size of metadata (including length), detected metadata).
        // Size of metadata can be found using https://playground.sourcify.dev/
        let test_vector = [
            (
                "ipfs",
                ipfs_bytecode,
                51usize + 2,
                Some(DetectedMetadata::Cbor {
                    full_length: 53,
                    metadata: CborMetadata {
                        ipfs: Some(vec![
                            18, 32, 188, 168, 70, 219, 54, 43, 98, 210, 235, 152, 145, 86, 91, 18,
                            67, 52, 16, 224, 246, 166, 52, 101, 125, 44, 125, 30, 116, 105, 68,
                            126, 138, 181,
                        ]),
                        bzzr1: None,
                        bzzr0: None,
                        experimental: None,
                        solc: Some(CborCompilerVersion::Native(vec![0, 8, 28])),
                        vyper: None,
                    },
                }),
            ),
            (
                "none",
                none_bytecode,
                10 + 2,
                Some(DetectedMetadata::Cbor {
                    full_length: 12,
                    metadata: CborMetadata {
                        ipfs: None,
                        bzzr1: None,
                        bzzr0: None,
                        experimental: None,
                        solc: Some(CborCompilerVersion::Native(vec![0, 8, 28])),
                        vyper: None,
                    },
                }),
            ),
            (
                "swarm",
                swarm_bytecode,
                50 + 2,
                Some(DetectedMetadata::Cbor {
                    full_length: 52,
                    metadata: CborMetadata {
                        ipfs: None,
                        bzzr1: Some(vec![
                            192, 222, 243, 12, 87, 22, 110, 151, 214, 165, 130, 144, 33, 63, 59,
                            13, 31, 131, 83, 46, 122, 3, 113, 200, 226, 182, 219, 168, 38, 186,
                            228, 97,
                        ]),
                        bzzr0: None,
                        experimental: None,
                        solc: Some(CborCompilerVersion::Native(vec![0, 8, 28])),
                        vyper: None,
                    },
                }),
            ),
        ];

        for (label, bytecode, full_metadata_len, metadata) in test_vector {
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
                identifier.detected_metadata, metadata,
                "{label}: Incorrect detected metadata"
            );
            assert_eq!(
                identifier.bytecode_without_metadata_keccak256, bytecode_without_metadata_keccak256,
                "{label}: Incorrect bytecode without metadata hash"
            );
        }
    }

    #[test]
    fn vyper_none() {
        // Sample contract with no methods, compiled from the root of monorepo with:
        // ./etc/vyper-bin/0.3.10/vyper test.vy --no-bytecode-metadata
        // (Use `zkstack contract-verifier init` to download compilers)
        let data = hex::decode("61002761000f6000396100276000f35f3560e01c6398716725811861001f5734610023575f5460405260206040f35b5f5ffd5b5f80fd").unwrap();
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
    fn vyper_keccak_without_padding() {
        // Sample contract with no methods, compiled from the root of monorepo with:
        // ./etc/zkvyper-bin/v1.5.4/zkvyper --vyper ./etc/vyper-bin/0.3.10/vyper test.vy
        // (Use `zkstack contract-verifier init` to download compilers)
        let data = hex::decode("0000000100200190000000100000c13d00000000020100190000000800200198000000150000613d000000000101043b00000009011001970000000a0010009c000000150000c13d0000000001000416000000000001004b000000150000c13d000000000100041a000000400010043f0000000b01000041000000180001042e0000002001000039000001000010044300000120000004430000000701000041000000180001042e000000000100001900000019000104300000001700000432000000180001042e000000190001043000000000000000000000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000fffffffc000000000000000000000000ffffffff0000000000000000000000000000000000000000000000000000000098716725000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000400000000000000000e80a9cfd35cba3f2346174b8eda3178b883727aff1778ac11ca8411a8e6fbae4").unwrap();
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
    fn zkvyper_cbor() {
        // ./etc/zkvyper-bin/v1.5.10/zkvyper --vyper ./etc/vyper-bin/0.3.10/vyper --metadata-hash none test.vy
        let none_bytecode = "00000001002001900000000f0000c13d0000000800100198000000140000613d000000000101043b00000009011001970000000a0010009c000000140000c13d0000000001000416000000000001004b000000140000c13d000000000100041a000000400010043f0000000b01000041000000170001042e0000002001000039000001000010044300000120000004430000000701000041000000170001042e000000000100001900000018000104300000001600000432000000170001042e0000001800010430000000000000000000000000000000000000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000fffffffc000000000000000000000000ffffffff000000000000000000000000000000000000000000000000000000009871672500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002000000040000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a1657679706572781b7a6b76797065723a312e352e31303b76797065723a302e332e31300024";

        // ./etc/zkvyper-bin/v1.5.10/zkvyper --vyper ./etc/vyper-bin/0.3.10/vyper --metadata-hash ipfs test.vy
        let ipfs_bytecode = "00000001002001900000000f0000c13d0000000800100198000000140000613d000000000101043b00000009011001970000000a0010009c000000140000c13d0000000001000416000000000001004b000000140000c13d000000000100041a000000400010043f0000000b01000041000000170001042e0000002001000039000001000010044300000120000004430000000701000041000000170001042e000000000100001900000018000104300000001600000432000000170001042e0000001800010430000000000000000000000000000000000000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000fffffffc000000000000000000000000ffffffff00000000000000000000000000000000000000000000000000000000987167250000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000004000000000000000000000000000000000000000000000000000a264697066735822122027a39f09f1e84c0633aef5d8768191da6ec0ad423dd55c63e107bb831bbab1e4657679706572781b7a6b76797065723a312e352e31303b76797065723a302e332e3130004d";

        // // Different variations of the same contract, compiled with different metadata options.
        let test_vector = [
            (
                "none",
                none_bytecode,
                36usize + 2,
                Some(DetectedMetadata::Cbor {
                    full_length: 38,
                    metadata: CborMetadata {
                        ipfs: None,
                        bzzr1: None,
                        bzzr0: None,
                        experimental: None,
                        solc: None,
                        vyper: Some(CborCompilerVersion::ZKsync(
                            "zkvyper:1.5.10;vyper:0.3.10".to_string(),
                        )),
                    },
                }),
            ),
            (
                "ipfs",
                ipfs_bytecode,
                77usize + 2,
                Some(DetectedMetadata::Cbor {
                    full_length: 79,
                    metadata: CborMetadata {
                        ipfs: Some(vec![
                            18, 32, 39, 163, 159, 9, 241, 232, 76, 6, 51, 174, 245, 216, 118, 129,
                            145, 218, 110, 192, 173, 66, 61, 213, 92, 99, 225, 7, 187, 131, 27,
                            186, 177, 228,
                        ]),
                        bzzr1: None,
                        bzzr0: None,
                        experimental: None,
                        solc: None,
                        vyper: Some(CborCompilerVersion::ZKsync(
                            "zkvyper:1.5.10;vyper:0.3.10".to_string(),
                        )),
                    },
                }),
            ),
        ];

        for (label, bytecode, full_metadata_len, metadata) in test_vector {
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
                identifier.detected_metadata, metadata,
                "{label}: Incorrect detected metadata"
            );
            assert_eq!(
                identifier.bytecode_without_metadata_keccak256, bytecode_without_metadata_keccak256,
                "{label}: Incorrect bytecode without metadata hash"
            );
        }
    }

    #[test]
    fn cbor_compiler_version_native_compiler_version_valid() {
        let version = CborCompilerVersion::Native(vec![0, 8, 28]);
        let (compiler_version, zk_compiler_version) = version.get_compiler_versions();
        assert_eq!(compiler_version, Some("0.8.28".to_string()));
        assert_eq!(zk_compiler_version, None);
    }

    #[test]
    fn cbor_compiler_version_native_compiler_version_invalid() {
        let version = CborCompilerVersion::Native(vec![0, 8]);
        let (compiler_version, zk_compiler_version) = version.get_compiler_versions();
        assert_eq!(compiler_version, None);
        assert_eq!(zk_compiler_version, None);
    }

    #[test]
    fn cbor_compiler_version_zksync_compiler_version_single() {
        let version = CborCompilerVersion::ZKsync("zksolc:1.5.13".to_string());
        let (compiler_version, zk_compiler_version) = version.get_compiler_versions();
        assert_eq!(compiler_version, None);
        assert_eq!(zk_compiler_version, Some("v1.5.13".to_string()));
    }

    #[test]
    fn cbor_compiler_version_zksync_compiler_version_dual() {
        let version = CborCompilerVersion::ZKsync("zkvyper:1.5.10;vyper:0.4.1".to_string());
        let (compiler_version, zk_compiler_version) = version.get_compiler_versions();
        assert_eq!(compiler_version, Some("0.4.1".to_string()));
        assert_eq!(zk_compiler_version, Some("v1.5.10".to_string()));
    }

    #[test]
    fn cbor_compiler_version_zksync_compiler_version_triple() {
        let version =
            CborCompilerVersion::ZKsync("zksolc:1.5.13;solc:0.8.29;llvm:1.0.2".to_string());
        let (compiler_version, zk_compiler_version) = version.get_compiler_versions();
        assert_eq!(compiler_version, Some("zkVM-0.8.29-1.0.2".to_string()));
        assert_eq!(zk_compiler_version, Some("v1.5.13".to_string()));
    }

    #[test]
    fn cbor_compiler_version_zksync_compiler_version_invalid() {
        let version = CborCompilerVersion::ZKsync("invalid_format".to_string());
        let (compiler_version, zk_compiler_version) = version.get_compiler_versions();
        assert_eq!(compiler_version, None);
        assert_eq!(zk_compiler_version, None);
    }

    #[test]
    fn cbor_metadata_get_compiler_versions_solc() {
        let metadata = CborMetadata {
            solc: Some(CborCompilerVersion::Native(vec![0, 8, 28])),
            vyper: None,
            ..Default::default()
        };
        let (compiler_version, zk_compiler_version) = metadata.get_compiler_versions();
        assert_eq!(compiler_version, Some("0.8.28".to_string()));
        assert_eq!(zk_compiler_version, None);
    }

    #[test]
    fn cbor_metadata_get_compiler_versions_vyper() {
        let metadata = CborMetadata {
            solc: None,
            vyper: Some(CborCompilerVersion::ZKsync(
                "zkvyper:1.5.10;vyper:0.4.1".to_string(),
            )),
            ..Default::default()
        };
        let (compiler_version, zk_compiler_version) = metadata.get_compiler_versions();
        assert_eq!(compiler_version, Some("0.4.1".to_string()));
        assert_eq!(zk_compiler_version, Some("v1.5.10".to_string()));
    }

    #[test]
    fn cbor_metadata_get_compiler_versions_none_present() {
        let metadata = CborMetadata {
            solc: None,
            vyper: None,
            ..Default::default()
        };
        let (compiler_version, zk_compiler_version) = metadata.get_compiler_versions();
        assert_eq!(compiler_version, None);
        assert_eq!(zk_compiler_version, None);
    }

    #[test]
    fn cbor_metadata_get_compiler_versions_invalid_solc() {
        let metadata = CborMetadata {
            solc: Some(CborCompilerVersion::Native(vec![0, 8])),
            vyper: None,
            ..Default::default()
        };
        let (compiler_version, zk_compiler_version) = metadata.get_compiler_versions();
        assert_eq!(compiler_version, None);
        assert_eq!(zk_compiler_version, None);
    }

    #[test]
    fn cbor_metadata_get_compiler_versions_invalid_vyper() {
        let metadata = CborMetadata {
            solc: None,
            vyper: Some(CborCompilerVersion::ZKsync("invalid_format".to_string())),
            ..Default::default()
        };
        let (compiler_version, zk_compiler_version) = metadata.get_compiler_versions();
        assert_eq!(compiler_version, None);
        assert_eq!(zk_compiler_version, None);
    }
}
