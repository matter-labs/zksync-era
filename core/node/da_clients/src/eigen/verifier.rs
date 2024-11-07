use std::{collections::HashMap, str::FromStr};

use alloy::{
    network::Ethereum,
    providers::{Provider, RootProvider},
};
use ark_bn254::{Fq, G1Affine};
use ethabi::{encode, Token};
use rust_kzg_bn254::{blob::Blob, kzg::Kzg, polynomial::PolynomialFormat};
use tiny_keccak::{Hasher, Keccak};

use super::{
    blob_info::{BatchHeader, BlobHeader, BlobInfo, G1Commitment},
    generated::eigendaservicemanager::EigenDAServiceManager,
};

#[derive(Debug)]
pub enum VerificationError {
    ServiceManagerError,
    WrongUrl,
    KzgError,
    WrongProof,
    DifferentCommitments,
    DifferentRoots,
    EmptyHash,
    DifferentHashes,
    WrongQuorumParams,
    QuorumNotConfirmed,
    CommitmentNotOnCurve,
    CommitmentNotOnCorrectSubgroup,
}

/// Configuration for the verifier used for authenticated dispersals
#[derive(Debug, Clone)]
pub struct VerifierConfig {
    pub verify_certs: bool,
    pub rpc_url: String,
    pub svc_manager_addr: String,
    pub max_blob_size: u32,
    pub path_to_points: String,
    pub eth_confirmation_depth: u32,
}

/// Verifier used to verify the integrity of the blob info
/// Kzg is used for commitment verification
/// EigenDA service manager is used to connect to the service manager contract
#[derive(Debug, Clone)]
pub struct Verifier {
    kzg: Kzg,
    eigenda_svc_manager: EigenDAServiceManager::EigenDAServiceManagerInstance<
        alloy::transports::http::Http<alloy::transports::http::Client>,
        RootProvider<alloy::transports::http::Http<alloy::transports::http::Client>>,
    >,
    cfg: VerifierConfig,
}

impl Verifier {
    pub fn new(cfg: VerifierConfig) -> Result<Self, VerificationError> {
        let srs_points_to_load = cfg.max_blob_size / 32;
        let kzg = Kzg::setup(
            &format!("{}{}", cfg.path_to_points, "/g1.point"),
            "",
            &format!("{}{}", cfg.path_to_points, "/g2.point.powerOf2"),
            268435456, // 2 ^ 28
            srs_points_to_load,
            "".to_string(),
        );
        let kzg = kzg.map_err(|e| {
            tracing::error!("Failed to setup KZG: {:?}", e);
            VerificationError::KzgError
        })?;
        let url = alloy::transports::http::reqwest::Url::from_str(&cfg.rpc_url)
            .map_err(|_| VerificationError::WrongUrl)?;
        let provider: RootProvider<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            Ethereum,
        > = RootProvider::new_http(url);

        let svc_manager_addr = alloy::primitives::Address::from_str(&cfg.svc_manager_addr)
            .map_err(|_| VerificationError::ServiceManagerError)?;

        let eigenda_svc_manager = EigenDAServiceManager::new(svc_manager_addr, provider);
        Ok(Self {
            kzg,
            eigenda_svc_manager,
            cfg,
        })
    }

    /// Return the commitment from a blob
    fn commit(&self, blob: Vec<u8>) -> Result<G1Affine, VerificationError> {
        let blob = Blob::from_bytes_and_pad(&blob.to_vec());
        self.kzg
            .blob_to_kzg_commitment(&blob, PolynomialFormat::InEvaluationForm)
            .map_err(|_| VerificationError::KzgError)
    }

    /// Compare the given commitment with the commitment generated with the blob
    pub fn verify_commitment(
        &self,
        expected_commitment: G1Commitment,
        blob: Vec<u8>,
    ) -> Result<(), VerificationError> {
        let actual_commitment = self.commit(blob)?;
        let expected_commitment = G1Affine::new_unchecked(
            Fq::from(num_bigint::BigUint::from_bytes_be(&expected_commitment.x)),
            Fq::from(num_bigint::BigUint::from_bytes_be(&expected_commitment.y)),
        );
        if !expected_commitment.is_on_curve() {
            return Err(VerificationError::CommitmentNotOnCurve);
        }
        if !expected_commitment.is_in_correct_subgroup_assuming_on_curve() {
            return Err(VerificationError::CommitmentNotOnCorrectSubgroup);
        }
        if actual_commitment != expected_commitment {
            return Err(VerificationError::DifferentCommitments);
        }
        Ok(())
    }

    fn hash_encode_blob_header(&self, blob_header: BlobHeader) -> Vec<u8> {
        let mut blob_quorums = vec![];
        for quorum in blob_header.blob_quorum_params {
            let quorum = Token::Tuple(vec![
                Token::Uint(ethabi::Uint::from(quorum.quorum_number)),
                Token::Uint(ethabi::Uint::from(quorum.adversary_threshold_percentage)),
                Token::Uint(ethabi::Uint::from(quorum.confirmation_threshold_percentage)),
                Token::Uint(ethabi::Uint::from(quorum.chunk_length)),
            ]);
            blob_quorums.push(quorum);
        }
        let blob_header = Token::Tuple(vec![
            Token::Tuple(vec![
                Token::Uint(ethabi::Uint::from_big_endian(&blob_header.commitment.x)),
                Token::Uint(ethabi::Uint::from_big_endian(&blob_header.commitment.y)),
            ]),
            Token::Uint(ethabi::Uint::from(blob_header.data_length)),
            Token::Array(blob_quorums),
        ]);

        let encoded = encode(&[blob_header]);

        let mut keccak = Keccak::v256();
        keccak.update(&encoded);
        let mut hash = [0u8; 32];
        keccak.finalize(&mut hash);
        hash.to_vec()
    }

    fn process_inclusion_proof(
        &self,
        proof: &[u8],
        leaf: &[u8],
        index: u32,
    ) -> Result<Vec<u8>, VerificationError> {
        let mut index = index;
        if proof.is_empty() || proof.len() % 32 != 0 {
            return Err(VerificationError::WrongProof);
        }
        let mut computed_hash = leaf.to_vec();
        for i in 0..proof.len() / 32 {
            let mut combined = proof[i * 32..(i + 1) * 32]
                .iter()
                .chain(computed_hash.iter())
                .cloned()
                .collect::<Vec<u8>>();
            if index % 2 == 0 {
                combined = computed_hash
                    .iter()
                    .chain(proof[i * 32..(i + 1) * 32].iter())
                    .cloned()
                    .collect::<Vec<u8>>();
            };
            let mut keccak = Keccak::v256();
            keccak.update(&combined);
            let mut hash = [0u8; 32];
            keccak.finalize(&mut hash);
            computed_hash = hash.to_vec();
            index /= 2;
        }

        Ok(computed_hash)
    }

    /// Verifies the certificate's batch root
    fn verify_merkle_proof(&self, cert: BlobInfo) -> Result<(), VerificationError> {
        let inclusion_proof = cert.blob_verification_proof.inclusion_proof;
        let root = cert
            .blob_verification_proof
            .batch_medatada
            .batch_header
            .batch_root;
        let blob_index = cert.blob_verification_proof.blob_index;
        let blob_header = cert.blob_header;

        let blob_header_hash = self.hash_encode_blob_header(blob_header);
        let mut keccak = Keccak::v256();
        keccak.update(&blob_header_hash);
        let mut leaf_hash = [0u8; 32];
        keccak.finalize(&mut leaf_hash);

        let generated_root =
            self.process_inclusion_proof(&inclusion_proof, &leaf_hash, blob_index)?;

        if generated_root != root {
            return Err(VerificationError::DifferentRoots);
        }
        Ok(())
    }

    fn hash_batch_metadata(
        &self,
        batch_header: BatchHeader,
        signatory_record_hash: Vec<u8>,
        confirmation_block_number: u32,
    ) -> Vec<u8> {
        let batch_header_token = Token::Tuple(vec![
            Token::FixedBytes(batch_header.batch_root),
            Token::Bytes(batch_header.quorum_numbers),
            Token::Bytes(batch_header.quorum_signed_percentages),
            Token::Uint(ethabi::Uint::from(batch_header.reference_block_number)),
        ]);

        let encoded = encode(&[batch_header_token]);

        let mut keccak = Keccak::v256();
        keccak.update(&encoded);
        let mut header_hash = [0u8; 32];
        keccak.finalize(&mut header_hash);

        let hash_token = Token::Tuple(vec![
            Token::FixedBytes(header_hash.to_vec()),
            Token::FixedBytes(signatory_record_hash),
        ]);

        let mut hash_encoded = encode(&[hash_token]);

        hash_encoded.append(&mut confirmation_block_number.to_be_bytes().to_vec());

        let mut keccak = Keccak::v256();
        keccak.update(&hash_encoded);
        let mut hash = [0u8; 32];
        keccak.finalize(&mut hash);

        hash.to_vec()
    }

    /// Retrieves the block to make the request to the service manager
    async fn get_context_block(&self) -> Result<u64, VerificationError> {
        let latest = self
            .eigenda_svc_manager
            .provider()
            .get_block_number()
            .await
            .map_err(|_| VerificationError::ServiceManagerError)?;

        if self.cfg.eth_confirmation_depth == 0 {
            return Ok(latest);
        }
        Ok(latest - (self.cfg.eth_confirmation_depth as u64 - 1))
    }

    /// Verifies the certificate batch hash
    async fn verify_batch(&self, cert: BlobInfo) -> Result<(), VerificationError> {
        let context_block = self.get_context_block().await?;
        let expected_hash = self
            .eigenda_svc_manager
            .batchIdToBatchMetadataHash(cert.blob_verification_proof.batch_id)
            .block(context_block.into())
            .call()
            .await
            .map_err(|_| VerificationError::ServiceManagerError)?
            ._0
            .to_vec();

        if expected_hash == vec![0u8; 32] {
            return Err(VerificationError::EmptyHash);
        }

        let actual_hash = self.hash_batch_metadata(
            cert.blob_verification_proof.batch_medatada.batch_header,
            cert.blob_verification_proof
                .batch_medatada
                .signatory_record_hash,
            cert.blob_verification_proof
                .batch_medatada
                .confirmation_block_number,
        );

        if expected_hash != actual_hash {
            return Err(VerificationError::DifferentHashes);
        }
        Ok(())
    }

    async fn get_quorum_adversary_threshold(
        &self,
        quorum_number: u32,
    ) -> Result<u8, VerificationError> {
        let percentages = self
            .eigenda_svc_manager
            .quorumAdversaryThresholdPercentages()
            .call()
            .await
            .map_err(|_| VerificationError::ServiceManagerError)?
            ._0;
        if percentages.len() > quorum_number as usize {
            return Ok(percentages[quorum_number as usize]);
        }
        Ok(0)
    }

    /// Verifies that the certificate's blob quorum params are correct
    async fn verify_security_params(&self, cert: BlobInfo) -> Result<(), VerificationError> {
        let blob_header = cert.blob_header;
        let batch_header = cert.blob_verification_proof.batch_medatada.batch_header;

        let mut confirmed_quorums: HashMap<u32, bool> = HashMap::new();
        for i in 0..blob_header.blob_quorum_params.len() {
            if batch_header.quorum_numbers[i] as u32
                != blob_header.blob_quorum_params[i].quorum_number
            {
                return Err(VerificationError::WrongQuorumParams);
            }
            if blob_header.blob_quorum_params[i].adversary_threshold_percentage
                > blob_header.blob_quorum_params[i].confirmation_threshold_percentage
            {
                return Err(VerificationError::WrongQuorumParams);
            }
            let quorum_adversary_threshold = self
                .get_quorum_adversary_threshold(blob_header.blob_quorum_params[i].quorum_number)
                .await?;

            if quorum_adversary_threshold > 0
                && blob_header.blob_quorum_params[i].adversary_threshold_percentage
                    < quorum_adversary_threshold as u32
            {
                return Err(VerificationError::WrongQuorumParams);
            }

            if (batch_header.quorum_signed_percentages[i] as u32)
                < blob_header.blob_quorum_params[i].confirmation_threshold_percentage
            {
                return Err(VerificationError::WrongQuorumParams);
            }

            confirmed_quorums.insert(blob_header.blob_quorum_params[i].quorum_number, true);
        }

        let required_quorums = self
            .eigenda_svc_manager
            .quorumNumbersRequired()
            .call()
            .await
            .map_err(|_| VerificationError::ServiceManagerError)?
            ._0
            .to_vec();

        for quorum in required_quorums {
            if !confirmed_quorums.contains_key(&(quorum as u32)) {
                return Err(VerificationError::QuorumNotConfirmed);
            }
        }
        Ok(())
    }

    /// Verifies that the certificate is valid
    pub async fn verify_certificate(&self, cert: BlobInfo) -> Result<(), VerificationError> {
        if !self.cfg.verify_certs {
            return Ok(());
        }
        self.verify_batch(cert.clone()).await?;
        self.verify_merkle_proof(cert.clone())?;
        self.verify_security_params(cert.clone()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::eigen::blob_info::{
        BatchHeader, BatchMetadata, BlobHeader, BlobInfo, BlobQuorumParam, BlobVerificationProof,
        G1Commitment,
    };

    #[test]
    fn test_verify_commitment() {
        let verifier = super::Verifier::new(super::VerifierConfig {
            verify_certs: true,
            rpc_url: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            svc_manager_addr: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            max_blob_size: 2 * 1024 * 1024,
            path_to_points: "../../../resources".to_string(),
            eth_confirmation_depth: 0,
        })
        .unwrap();
        let commitment = G1Commitment {
            x: vec![
                22, 11, 176, 29, 82, 48, 62, 49, 51, 119, 94, 17, 156, 142, 248, 96, 240, 183, 134,
                85, 152, 5, 74, 27, 175, 83, 162, 148, 17, 110, 201, 74,
            ],
            y: vec![
                12, 132, 236, 56, 147, 6, 176, 135, 244, 166, 21, 18, 87, 76, 122, 3, 23, 22, 254,
                236, 148, 129, 110, 207, 131, 116, 58, 170, 4, 130, 191, 157,
            ],
        };
        let blob = vec![1u8; 100]; // Actual blob sent was this blob but kzg-padded, but Blob::from_bytes_and_pad padds it inside, so we don't need to pad it here.
        let result = verifier.verify_commitment(commitment, blob);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_merkle_proof() {
        let verifier = super::Verifier::new(super::VerifierConfig {
            verify_certs: true,
            rpc_url: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            svc_manager_addr: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            max_blob_size: 2 * 1024 * 1024,
            path_to_points: "../../../resources".to_string(),
            eth_confirmation_depth: 0,
        })
        .unwrap();
        let cert = BlobInfo {
            blob_header: BlobHeader {
                commitment: G1Commitment {
                    x: vec![
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ],
                    y: vec![
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ],
                },
                data_length: 4,
                blob_quorum_params: vec![
                    BlobQuorumParam {
                        quorum_number: 0,
                        adversary_threshold_percentage: 33,
                        confirmation_threshold_percentage: 55,
                        chunk_length: 1,
                    },
                    BlobQuorumParam {
                        quorum_number: 1,
                        adversary_threshold_percentage: 33,
                        confirmation_threshold_percentage: 55,
                        chunk_length: 1,
                    },
                ],
            },
            blob_verification_proof: BlobVerificationProof {
                batch_id: 66507,
                blob_index: 92,
                batch_medatada: BatchMetadata {
                    batch_header: BatchHeader {
                        batch_root: vec![
                            179, 187, 53, 98, 192, 80, 151, 28, 125, 192, 115, 29, 129, 238, 216,
                            8, 213, 210, 203, 143, 181, 19, 146, 113, 98, 131, 39, 238, 149, 248,
                            211, 43,
                        ],
                        quorum_numbers: vec![0, 1],
                        quorum_signed_percentages: vec![100, 100],
                        reference_block_number: 2624794,
                    },
                    signatory_record_hash: vec![
                        172, 32, 172, 142, 197, 52, 84, 143, 120, 26, 190, 9, 143, 217, 62, 19, 17,
                        107, 105, 67, 203, 5, 172, 249, 6, 60, 105, 240, 134, 34, 66, 133,
                    ],
                    fee: vec![0],
                    confirmation_block_number: 2624876,
                    batch_header_hash: vec![
                        122, 115, 2, 85, 233, 75, 121, 85, 51, 81, 248, 170, 198, 252, 42, 16, 1,
                        146, 96, 218, 159, 44, 41, 40, 94, 247, 147, 11, 255, 68, 40, 177,
                    ],
                },
                inclusion_proof: vec![
                    203, 160, 237, 48, 117, 255, 75, 254, 117, 144, 164, 77, 29, 146, 36, 48, 190,
                    140, 50, 100, 144, 237, 125, 125, 75, 54, 210, 247, 147, 23, 48, 189, 120, 4,
                    125, 123, 195, 244, 207, 239, 145, 109, 0, 21, 11, 162, 109, 79, 192, 100, 138,
                    157, 203, 22, 17, 114, 234, 72, 174, 231, 209, 133, 99, 118, 201, 160, 137,
                    128, 112, 84, 34, 136, 174, 139, 96, 26, 246, 148, 134, 52, 200, 229, 160, 145,
                    5, 120, 18, 187, 51, 11, 109, 91, 237, 171, 215, 207, 90, 95, 146, 54, 135,
                    166, 66, 157, 255, 237, 69, 183, 141, 45, 162, 145, 71, 16, 87, 184, 120, 84,
                    156, 220, 159, 4, 99, 48, 191, 203, 136, 112, 127, 226, 192, 184, 110, 6, 177,
                    182, 109, 207, 197, 239, 161, 132, 17, 89, 56, 137, 205, 202, 101, 97, 60, 162,
                    253, 23, 169, 75, 236, 211, 126, 121, 132, 191, 68, 167, 200, 16, 154, 149,
                    202, 197, 7, 191, 26, 8, 67, 3, 37, 137, 16, 153, 30, 209, 238, 53, 233, 148,
                    198, 253, 94, 216, 73, 25, 190, 205, 132, 208, 255, 219, 170, 98, 17, 160, 179,
                    183, 200, 17, 99, 36, 130, 216, 223, 72, 222, 250, 73, 78, 79, 72, 253, 105,
                    245, 84, 244, 196,
                ],
                quorum_indexes: vec![0, 1],
            },
        };
        let result = verifier.verify_merkle_proof(cert);
        assert!(result.is_ok());
    }

    #[test]
    fn test_hash_blob_header() {
        let verifier = super::Verifier::new(super::VerifierConfig {
            verify_certs: true,
            rpc_url: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            svc_manager_addr: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            max_blob_size: 2 * 1024 * 1024,
            path_to_points: "../../../resources".to_string(),
            eth_confirmation_depth: 0,
        })
        .unwrap();
        let blob_header = BlobHeader {
            commitment: G1Commitment {
                x: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 1,
                ],
                y: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 1,
                ],
            },
            data_length: 2,
            blob_quorum_params: vec![
                BlobQuorumParam {
                    quorum_number: 2,
                    adversary_threshold_percentage: 4,
                    confirmation_threshold_percentage: 5,
                    chunk_length: 6,
                },
                BlobQuorumParam {
                    quorum_number: 2,
                    adversary_threshold_percentage: 4,
                    confirmation_threshold_percentage: 5,
                    chunk_length: 6,
                },
            ],
        };
        let result = verifier.hash_encode_blob_header(blob_header);
        let expected = "ba4675a31c9bf6b2f7abfdcedd34b74645cb7332b35db39bff00ae8516a67393";
        assert_eq!(result, hex::decode(expected).unwrap());
    }

    #[test]
    fn test_inclusion_proof() {
        let verifier = super::Verifier::new(super::VerifierConfig {
            verify_certs: true,
            rpc_url: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            svc_manager_addr: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            max_blob_size: 2 * 1024 * 1024,
            path_to_points: "../../../resources".to_string(),
            eth_confirmation_depth: 0,
        })
        .unwrap();

        let proof = hex::decode("c455c1ea0e725d7ea3e5f29e9f48be8fc2787bb0a914d5a86710ba302c166ac4f626d76f67f1055bb960a514fb8923af2078fd84085d712655b58a19612e8cd15c3e4ac1cef57acde3438dbcf63f47c9fefe1221344c4d5c1a4943dd0d1803091ca81a270909dc0e146841441c9bd0e08e69ce6168181a3e4060ffacf3627480bec6abdd8d7bb92b49d33f180c42f49e041752aaded9c403db3a17b85e48a11e9ea9a08763f7f383dab6d25236f1b77c12b4c49c5cdbcbea32554a604e3f1d2f466851cb43fe73617b3d01e665e4c019bf930f92dea7394c25ed6a1e200d051fb0c30a2193c459f1cfef00bf1ba6656510d16725a4d1dc031cb759dbc90bab427b0f60ddc6764681924dda848824605a4f08b7f526fe6bd4572458c94e83fbf2150f2eeb28d3011ec921996dc3e69efa52d5fcf3182b20b56b5857a926aa66605808079b4d52c0c0cfe06923fa92e65eeca2c3e6126108e8c1babf5ac522f4d7").unwrap();
        let leaf = hex::decode("f6106e6ae4631e68abe0fa898cedbe97dbae6c7efb1b088c5aa2e8b91190ff96")
            .unwrap();
        let expected_root =
            hex::decode("7390b8023db8248123dcaeca57fa6c9340bef639e204f2278fc7ec3d46ad071b")
                .unwrap();

        let actual_root = verifier
            .process_inclusion_proof(&proof, &leaf, 580)
            .unwrap();

        assert_eq!(actual_root, expected_root);
    }

    #[tokio::test]
    async fn test_verify_batch() {
        let verifier = super::Verifier::new(super::VerifierConfig {
            verify_certs: true,
            rpc_url: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            svc_manager_addr: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            max_blob_size: 2 * 1024 * 1024,
            path_to_points: "../../../resources".to_string(),
            eth_confirmation_depth: 0,
        })
        .unwrap();
        let cert = BlobInfo {
            blob_header: BlobHeader {
                commitment: G1Commitment {
                    x: vec![
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ],
                    y: vec![
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ],
                },
                data_length: 4,
                blob_quorum_params: vec![
                    BlobQuorumParam {
                        quorum_number: 0,
                        adversary_threshold_percentage: 33,
                        confirmation_threshold_percentage: 55,
                        chunk_length: 1,
                    },
                    BlobQuorumParam {
                        quorum_number: 1,
                        adversary_threshold_percentage: 33,
                        confirmation_threshold_percentage: 55,
                        chunk_length: 1,
                    },
                ],
            },
            blob_verification_proof: BlobVerificationProof {
                batch_id: 66507,
                blob_index: 92,
                batch_medatada: BatchMetadata {
                    batch_header: BatchHeader {
                        batch_root: vec![
                            179, 187, 53, 98, 192, 80, 151, 28, 125, 192, 115, 29, 129, 238, 216,
                            8, 213, 210, 203, 143, 181, 19, 146, 113, 98, 131, 39, 238, 149, 248,
                            211, 43,
                        ],
                        quorum_numbers: vec![0, 1],
                        quorum_signed_percentages: vec![100, 100],
                        reference_block_number: 2624794,
                    },
                    signatory_record_hash: vec![
                        172, 32, 172, 142, 197, 52, 84, 143, 120, 26, 190, 9, 143, 217, 62, 19, 17,
                        107, 105, 67, 203, 5, 172, 249, 6, 60, 105, 240, 134, 34, 66, 133,
                    ],
                    fee: vec![0],
                    confirmation_block_number: 2624876,
                    batch_header_hash: vec![
                        122, 115, 2, 85, 233, 75, 121, 85, 51, 81, 248, 170, 198, 252, 42, 16, 1,
                        146, 96, 218, 159, 44, 41, 40, 94, 247, 147, 11, 255, 68, 40, 177,
                    ],
                },
                inclusion_proof: vec![
                    203, 160, 237, 48, 117, 255, 75, 254, 117, 144, 164, 77, 29, 146, 36, 48, 190,
                    140, 50, 100, 144, 237, 125, 125, 75, 54, 210, 247, 147, 23, 48, 189, 120, 4,
                    125, 123, 195, 244, 207, 239, 145, 109, 0, 21, 11, 162, 109, 79, 192, 100, 138,
                    157, 203, 22, 17, 114, 234, 72, 174, 231, 209, 133, 99, 118, 201, 160, 137,
                    128, 112, 84, 34, 136, 174, 139, 96, 26, 246, 148, 134, 52, 200, 229, 160, 145,
                    5, 120, 18, 187, 51, 11, 109, 91, 237, 171, 215, 207, 90, 95, 146, 54, 135,
                    166, 66, 157, 255, 237, 69, 183, 141, 45, 162, 145, 71, 16, 87, 184, 120, 84,
                    156, 220, 159, 4, 99, 48, 191, 203, 136, 112, 127, 226, 192, 184, 110, 6, 177,
                    182, 109, 207, 197, 239, 161, 132, 17, 89, 56, 137, 205, 202, 101, 97, 60, 162,
                    253, 23, 169, 75, 236, 211, 126, 121, 132, 191, 68, 167, 200, 16, 154, 149,
                    202, 197, 7, 191, 26, 8, 67, 3, 37, 137, 16, 153, 30, 209, 238, 53, 233, 148,
                    198, 253, 94, 216, 73, 25, 190, 205, 132, 208, 255, 219, 170, 98, 17, 160, 179,
                    183, 200, 17, 99, 36, 130, 216, 223, 72, 222, 250, 73, 78, 79, 72, 253, 105,
                    245, 84, 244, 196,
                ],
                quorum_indexes: vec![0, 1],
            },
        };
        let result = verifier.verify_batch(cert).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_verify_security_params() {
        let verifier = super::Verifier::new(super::VerifierConfig {
            verify_certs: true,
            rpc_url: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            svc_manager_addr: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            max_blob_size: 2 * 1024 * 1024,
            path_to_points: "../../../resources".to_string(),
            eth_confirmation_depth: 0,
        })
        .unwrap();
        let cert = BlobInfo {
            blob_header: BlobHeader {
                commitment: G1Commitment {
                    x: vec![
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ],
                    y: vec![
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ],
                },
                data_length: 4,
                blob_quorum_params: vec![
                    BlobQuorumParam {
                        quorum_number: 0,
                        adversary_threshold_percentage: 33,
                        confirmation_threshold_percentage: 55,
                        chunk_length: 1,
                    },
                    BlobQuorumParam {
                        quorum_number: 1,
                        adversary_threshold_percentage: 33,
                        confirmation_threshold_percentage: 55,
                        chunk_length: 1,
                    },
                ],
            },
            blob_verification_proof: BlobVerificationProof {
                batch_id: 66507,
                blob_index: 92,
                batch_medatada: BatchMetadata {
                    batch_header: BatchHeader {
                        batch_root: vec![
                            179, 187, 53, 98, 192, 80, 151, 28, 125, 192, 115, 29, 129, 238, 216,
                            8, 213, 210, 203, 143, 181, 19, 146, 113, 98, 131, 39, 238, 149, 248,
                            211, 43,
                        ],
                        quorum_numbers: vec![0, 1],
                        quorum_signed_percentages: vec![100, 100],
                        reference_block_number: 2624794,
                    },
                    signatory_record_hash: vec![
                        172, 32, 172, 142, 197, 52, 84, 143, 120, 26, 190, 9, 143, 217, 62, 19, 17,
                        107, 105, 67, 203, 5, 172, 249, 6, 60, 105, 240, 134, 34, 66, 133,
                    ],
                    fee: vec![0],
                    confirmation_block_number: 2624876,
                    batch_header_hash: vec![
                        122, 115, 2, 85, 233, 75, 121, 85, 51, 81, 248, 170, 198, 252, 42, 16, 1,
                        146, 96, 218, 159, 44, 41, 40, 94, 247, 147, 11, 255, 68, 40, 177,
                    ],
                },
                inclusion_proof: vec![
                    203, 160, 237, 48, 117, 255, 75, 254, 117, 144, 164, 77, 29, 146, 36, 48, 190,
                    140, 50, 100, 144, 237, 125, 125, 75, 54, 210, 247, 147, 23, 48, 189, 120, 4,
                    125, 123, 195, 244, 207, 239, 145, 109, 0, 21, 11, 162, 109, 79, 192, 100, 138,
                    157, 203, 22, 17, 114, 234, 72, 174, 231, 209, 133, 99, 118, 201, 160, 137,
                    128, 112, 84, 34, 136, 174, 139, 96, 26, 246, 148, 134, 52, 200, 229, 160, 145,
                    5, 120, 18, 187, 51, 11, 109, 91, 237, 171, 215, 207, 90, 95, 146, 54, 135,
                    166, 66, 157, 255, 237, 69, 183, 141, 45, 162, 145, 71, 16, 87, 184, 120, 84,
                    156, 220, 159, 4, 99, 48, 191, 203, 136, 112, 127, 226, 192, 184, 110, 6, 177,
                    182, 109, 207, 197, 239, 161, 132, 17, 89, 56, 137, 205, 202, 101, 97, 60, 162,
                    253, 23, 169, 75, 236, 211, 126, 121, 132, 191, 68, 167, 200, 16, 154, 149,
                    202, 197, 7, 191, 26, 8, 67, 3, 37, 137, 16, 153, 30, 209, 238, 53, 233, 148,
                    198, 253, 94, 216, 73, 25, 190, 205, 132, 208, 255, 219, 170, 98, 17, 160, 179,
                    183, 200, 17, 99, 36, 130, 216, 223, 72, 222, 250, 73, 78, 79, 72, 253, 105,
                    245, 84, 244, 196,
                ],
                quorum_indexes: vec![0, 1],
            },
        };
        let result = verifier.verify_security_params(cert).await;
        assert!(result.is_ok());
    }
}
