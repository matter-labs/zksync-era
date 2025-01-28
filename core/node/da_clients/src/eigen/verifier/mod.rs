use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use ark_bn254::{Fq, G1Affine};
use ethabi::{encode, ParamType, Token};
use rust_kzg_bn254::{blob::Blob, kzg::Kzg, polynomial::PolynomialFormat};
use tempfile::NamedTempFile;
use tokio::task::JoinHandle;
use zksync_basic_types::web3::CallRequest;
use zksync_config::EigenConfig;
use zksync_eth_client::EthInterface;
use zksync_types::{web3, Address, U256, U64};
use zksync_web3_decl::client::{DynClient, L1};

use super::{
    blob_info::{BatchHeader, BlobHeader, BlobInfo, G1Commitment},
    errors::{KzgError, ServiceManagerError, VerificationError},
    sdk::RawEigenClient,
};

#[cfg(test)]
mod tests;

fn decode_bytes(encoded: Vec<u8>) -> Result<Vec<u8>, VerificationError> {
    let output_type = [ParamType::Bytes];
    let tokens = ethabi::decode(&output_type, &encoded)
        .map_err(|e| ServiceManagerError::Decoding(e.to_string()))?;

    // Safe unwrap because decode guarantees type correctness and non-empty output
    let token = tokens.into_iter().next().unwrap();

    // Safe unwrap, as type is guaranteed
    Ok(token.into_bytes().unwrap())
}

#[async_trait::async_trait]
pub trait VerifierClient: Sync + Send + std::fmt::Debug {
    /// Request to the EigenDA service manager contract
    /// the batch metadata hash for a given batch id
    async fn batch_id_to_batch_metadata_hash(
        &self,
        batch_id: u32,
        svc_manager_addr: Address,
        settlement_layer_confirmation_depth: Option<U64>,
    ) -> Result<Vec<u8>, VerificationError>;

    /// Request to the EigenDA service manager contract
    /// the quorum adversary threshold percentages for a given quorum number
    async fn quorum_adversary_threshold_percentages(
        &self,
        quorum_number: u32,
        svc_manager_addr: Address,
    ) -> Result<u8, VerificationError>;

    /// Request to the EigenDA service manager contract
    /// the set of quorum numbers that are required
    async fn required_quorum_numbers(
        &self,
        svc_manager_addr: Address,
    ) -> Result<Vec<u8>, VerificationError>;
}

#[async_trait::async_trait]
impl VerifierClient for Box<DynClient<L1>> {
    async fn batch_id_to_batch_metadata_hash(
        &self,
        batch_id: u32,
        svc_manager_addr: Address,
        settlement_layer_confirmation_depth: Option<U64>,
    ) -> Result<Vec<u8>, VerificationError> {
        let mut data = vec![];
        let func_selector =
            ethabi::short_signature("batchIdToBatchMetadataHash", &[ParamType::Uint(32)]);
        data.extend_from_slice(&func_selector);
        let batch_id_data = encode(&[Token::Uint(U256::from(batch_id))]);
        data.extend_from_slice(&batch_id_data);

        let call_request = CallRequest {
            to: Some(svc_manager_addr),
            data: Some(zksync_basic_types::web3::Bytes(data)),
            ..Default::default()
        };

        let block_id = match settlement_layer_confirmation_depth {
            Some(depth) => {
                let depth = depth.saturating_sub(U64::one());
                let mut current_block = self
                    .block_number()
                    .await
                    .map_err(ServiceManagerError::EnrichedClient)?;
                current_block = current_block.saturating_sub(depth);
                Some(current_block.into())
            }
            None => None,
        };

        let res = self
            .as_ref()
            .call_contract_function(call_request, block_id)
            .await
            .map_err(ServiceManagerError::EnrichedClient)?;

        Ok(res.0.to_vec())
    }

    async fn quorum_adversary_threshold_percentages(
        &self,
        quorum_number: u32,
        svc_manager_addr: Address,
    ) -> Result<u8, VerificationError> {
        let func_selector = ethabi::short_signature("quorumAdversaryThresholdPercentages", &[]);
        let data = func_selector.to_vec();

        let call_request = CallRequest {
            to: Some(svc_manager_addr),
            data: Some(zksync_basic_types::web3::Bytes(data)),
            ..Default::default()
        };

        let res = self
            .as_ref()
            .call_contract_function(call_request, None)
            .await
            .map_err(ServiceManagerError::EnrichedClient)?;

        let percentages = decode_bytes(res.0)?;

        if percentages.len() > quorum_number as usize {
            return Ok(percentages[quorum_number as usize]);
        }
        Ok(0)
    }

    async fn required_quorum_numbers(
        &self,
        svc_manager_addr: Address,
    ) -> Result<Vec<u8>, VerificationError> {
        let func_selector = ethabi::short_signature("quorumNumbersRequired", &[]);
        let data = func_selector.to_vec();
        let call_request = CallRequest {
            to: Some(svc_manager_addr),
            data: Some(zksync_basic_types::web3::Bytes(data)),
            ..Default::default()
        };
        let res = self
            .as_ref()
            .call_contract_function(call_request, None)
            .await
            .map_err(ServiceManagerError::EnrichedClient)?;

        decode_bytes(res.0.to_vec())
    }
}

#[derive(Debug)]
enum PointFile {
    Temp(NamedTempFile),
    Path(PathBuf),
}

impl PointFile {
    fn path(&self) -> &Path {
        match self {
            PointFile::Temp(file) => file.path(),
            PointFile::Path(path) => path.as_path(),
        }
    }
}

/// Verifier used to verify the integrity of the blob info
/// Kzg is used for commitment verification
/// EigenDA service manager is used to connect to the service manager contract
#[derive(Debug)]
pub struct Verifier {
    kzg: Kzg,
    cfg: EigenConfig,
    client: Arc<dyn VerifierClient>,
}

impl Verifier {
    pub const DEFAULT_PRIORITY_FEE_PER_GAS: u64 = 100;
    pub const SRSORDER: u32 = 1 << 28; // 2 ^ 28
    pub const G1POINT: &'static str = "g1.point";
    pub const G2POINT: &'static str = "g2.point.powerOf2";
    pub const POINT_SIZE: u32 = 32;

    async fn download_temp_point(url: &String) -> Result<NamedTempFile, VerificationError> {
        let response = reqwest::get(url)
            .await
            .map_err(|e| VerificationError::PointDownloadError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(VerificationError::PointDownloadError(format!(
                "Failed to download point from source {}",
                url
            )));
        }

        let content = response
            .bytes()
            .await
            .map_err(|e| VerificationError::PointDownloadError(e.to_string()))?;

        // Tempfile writting uses `std::fs`, so we need to spawn a blocking task
        let temp_file = tokio::task::spawn_blocking(move || {
            let mut file = NamedTempFile::new()
                .map_err(|e| VerificationError::PointDownloadError(e.to_string()))?;

            file.write_all(&content)
                .map_err(|e| VerificationError::PointDownloadError(e.to_string()))?;

            file.flush()
                .map_err(|e| VerificationError::PointDownloadError(e.to_string()))?;

            Ok::<NamedTempFile, VerificationError>(file)
        })
        .await
        .map_err(|e| VerificationError::PointDownloadError(e.to_string()))??;

        Ok::<NamedTempFile, VerificationError>(temp_file)
    }

    async fn get_points(cfg: &EigenConfig) -> Result<(PointFile, PointFile), VerificationError> {
        match &cfg.points_dir {
            Some(path) => Ok((
                PointFile::Path(PathBuf::from(format!("{}/{}", path, Self::G1POINT))),
                PointFile::Path(PathBuf::from(format!("{}/{}", path, Self::G2POINT))),
            )),
            None => {
                tracing::info!("Points for KZG setup not found, downloading points to a temp file");
                Ok((
                    PointFile::Temp(Self::download_temp_point(&cfg.g1_url).await?),
                    PointFile::Temp(Self::download_temp_point(&cfg.g2_url).await?),
                ))
            }
        }
    }

    pub(crate) async fn new(
        cfg: EigenConfig,
        client: Arc<dyn VerifierClient>,
    ) -> Result<Self, VerificationError> {
        let srs_points_to_load = RawEigenClient::blob_size_limit() as u32 / Self::POINT_SIZE;
        let (g1_point_file, g2_point_file) = Self::get_points(&cfg).await?;
        let kzg_handle: JoinHandle<Result<Kzg, KzgError>> =
            tokio::task::spawn_blocking(move || {
                let g1_point_file_path = g1_point_file.path().to_str().ok_or(KzgError::Setup(
                    "Could not format point path into a valid string".to_string(),
                ))?;
                let g2_point_file_path = g2_point_file.path().to_str().ok_or(KzgError::Setup(
                    "Could not format point path into a valid string".to_string(),
                ))?;
                Kzg::setup(
                    g1_point_file_path,
                    "",
                    g2_point_file_path,
                    Self::SRSORDER,
                    srs_points_to_load,
                    "".to_string(),
                )
                .map_err(KzgError::Internal)
            });
        let kzg = kzg_handle
            .await
            .map_err(|e| VerificationError::Kzg(KzgError::Setup(e.to_string())))??;

        Ok(Self { kzg, cfg, client })
    }

    /// Return the commitment from a blob
    fn commit(&self, blob: &[u8]) -> Result<G1Affine, VerificationError> {
        let blob = Blob::from_bytes_and_pad(blob);
        self.kzg
            .blob_to_kzg_commitment(&blob, PolynomialFormat::InEvaluationForm)
            .map_err(|e| VerificationError::Kzg(KzgError::Internal(e)))
    }

    /// Compare the given commitment with the commitment generated with the blob
    pub fn verify_commitment(
        &self,
        expected_commitment: G1Commitment,
        blob: &[u8],
    ) -> Result<(), VerificationError> {
        let actual_commitment = self.commit(blob)?;
        let expected_commitment = G1Affine::new_unchecked(
            Fq::from(num_bigint::BigUint::from_bytes_be(&expected_commitment.x)),
            Fq::from(num_bigint::BigUint::from_bytes_be(&expected_commitment.y)),
        );
        if !expected_commitment.is_on_curve() {
            return Err(VerificationError::CommitmentNotOnCurve(expected_commitment));
        }
        if !expected_commitment.is_in_correct_subgroup_assuming_on_curve() {
            return Err(VerificationError::CommitmentNotOnCorrectSubgroup(
                expected_commitment,
            ));
        }
        if actual_commitment != expected_commitment {
            return Err(VerificationError::DifferentCommitments {
                expected: Box::new(expected_commitment),
                actual: Box::new(actual_commitment),
            });
        }
        Ok(())
    }

    pub(crate) fn hash_encode_blob_header(&self, blob_header: &BlobHeader) -> Vec<u8> {
        let mut blob_quorums = vec![];
        for quorum in &blob_header.blob_quorum_params {
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
        web3::keccak256(&encoded).to_vec()
    }

    pub(crate) fn process_inclusion_proof(
        &self,
        proof: &[u8],
        leaf: [u8; 32],
        index: u32,
    ) -> Result<Vec<u8>, VerificationError> {
        let mut index = index;
        if proof.is_empty() || proof.len() % 32 != 0 {
            return Err(VerificationError::WrongProof);
        }
        let mut computed_hash = leaf.to_vec();
        for chunk in proof.chunks(32) {
            let mut buffer = [0u8; 64];
            if index % 2 == 0 {
                buffer[..32].copy_from_slice(&computed_hash);
                buffer[32..].copy_from_slice(chunk);
            } else {
                buffer[..32].copy_from_slice(chunk);
                buffer[32..].copy_from_slice(&computed_hash);
            }
            computed_hash = web3::keccak256(&buffer).to_vec();
            index /= 2;
        }

        Ok(computed_hash)
    }

    /// Verifies the certificate's batch root
    pub(crate) fn verify_merkle_proof(&self, cert: &BlobInfo) -> Result<(), VerificationError> {
        let inclusion_proof = &cert.blob_verification_proof.inclusion_proof;
        let root = &cert
            .blob_verification_proof
            .batch_medatada
            .batch_header
            .batch_root;
        let blob_index = cert.blob_verification_proof.blob_index;
        let blob_header = &cert.blob_header;

        let blob_header_hash = self.hash_encode_blob_header(blob_header);
        let leaf_hash = web3::keccak256(&blob_header_hash);

        let generated_root =
            self.process_inclusion_proof(inclusion_proof, leaf_hash, blob_index)?;

        if generated_root != *root {
            return Err(VerificationError::DifferentRoots {
                expected: hex::encode(root),
                actual: hex::encode(&generated_root),
            });
        }
        Ok(())
    }

    fn hash_batch_metadata(
        &self,
        batch_header: &BatchHeader,
        signatory_record_hash: &[u8],
        confirmation_block_number: u32,
    ) -> Vec<u8> {
        let batch_header_token = Token::Tuple(vec![
            Token::FixedBytes(batch_header.batch_root.clone()), // Clone only where necessary
            Token::Bytes(batch_header.quorum_numbers.clone()),
            Token::Bytes(batch_header.quorum_signed_percentages.clone()),
            Token::Uint(ethabi::Uint::from(batch_header.reference_block_number)),
        ]);

        let encoded = encode(&[batch_header_token]);
        let header_hash = web3::keccak256(&encoded).to_vec();

        let hash_token = Token::Tuple(vec![
            Token::FixedBytes(header_hash.to_vec()),
            Token::FixedBytes(signatory_record_hash.to_owned()), // Clone only if required
        ]);

        let mut hash_encoded = encode(&[hash_token]);

        hash_encoded.append(&mut confirmation_block_number.to_be_bytes().to_vec());
        web3::keccak256(&hash_encoded).to_vec()
    }

    async fn call_batch_id_to_metadata_hash(
        &self,
        blob_info: &BlobInfo,
    ) -> Result<Vec<u8>, VerificationError> {
        self.client
            .as_ref()
            .batch_id_to_batch_metadata_hash(
                blob_info.blob_verification_proof.batch_id,
                self.cfg.eigenda_svc_manager_address,
                Some(U64::from(self.cfg.settlement_layer_confirmation_depth)),
            )
            .await
    }

    /// Verifies the certificate batch hash
    pub(crate) async fn verify_batch(&self, blob_info: &BlobInfo) -> Result<(), VerificationError> {
        let expected_hash = self.call_batch_id_to_metadata_hash(blob_info).await?;

        if expected_hash == vec![0u8; 32] {
            return Err(VerificationError::EmptyHash);
        }

        let actual_hash = self.hash_batch_metadata(
            &blob_info
                .blob_verification_proof
                .batch_medatada
                .batch_header,
            &blob_info
                .blob_verification_proof
                .batch_medatada
                .signatory_record_hash,
            blob_info
                .blob_verification_proof
                .batch_medatada
                .confirmation_block_number,
        );

        if expected_hash != actual_hash {
            return Err(VerificationError::DifferentHashes {
                expected: hex::encode(&expected_hash),
                actual: hex::encode(&actual_hash),
            });
        }
        Ok(())
    }

    async fn get_quorum_adversary_threshold(
        &self,
        quorum_number: u32,
    ) -> Result<u8, VerificationError> {
        self.client
            .as_ref()
            .quorum_adversary_threshold_percentages(
                quorum_number,
                self.cfg.eigenda_svc_manager_address,
            )
            .await
    }

    async fn call_quorum_numbers_required(&self) -> Result<Vec<u8>, VerificationError> {
        self.client
            .as_ref()
            .required_quorum_numbers(self.cfg.eigenda_svc_manager_address)
            .await
    }

    /// Verifies that the certificate's blob quorum params are correct
    pub async fn verify_security_params(&self, cert: &BlobInfo) -> Result<(), VerificationError> {
        let blob_header = &cert.blob_header;
        let batch_header = &cert.blob_verification_proof.batch_medatada.batch_header;

        let mut confirmed_quorums: HashMap<u32, bool> = HashMap::new();
        for i in 0..blob_header.blob_quorum_params.len() {
            if batch_header.quorum_numbers[i] as u32
                != blob_header.blob_quorum_params[i].quorum_number
            {
                return Err(VerificationError::WrongQuorumParams {
                    blob_quorum_params: blob_header.blob_quorum_params[i].clone(),
                });
            }
            if blob_header.blob_quorum_params[i].adversary_threshold_percentage
                > blob_header.blob_quorum_params[i].confirmation_threshold_percentage
            {
                return Err(VerificationError::WrongQuorumParams {
                    blob_quorum_params: blob_header.blob_quorum_params[i].clone(),
                });
            }
            let quorum_adversary_threshold = self
                .get_quorum_adversary_threshold(blob_header.blob_quorum_params[i].quorum_number)
                .await?;

            if quorum_adversary_threshold > 0
                && blob_header.blob_quorum_params[i].adversary_threshold_percentage
                    < quorum_adversary_threshold as u32
            {
                return Err(VerificationError::WrongQuorumParams {
                    blob_quorum_params: blob_header.blob_quorum_params[i].clone(),
                });
            }

            if (batch_header.quorum_signed_percentages[i] as u32)
                < blob_header.blob_quorum_params[i].confirmation_threshold_percentage
            {
                return Err(VerificationError::WrongQuorumParams {
                    blob_quorum_params: blob_header.blob_quorum_params[i].clone(),
                });
            }

            confirmed_quorums.insert(blob_header.blob_quorum_params[i].quorum_number, true);
        }

        let required_quorums = self.call_quorum_numbers_required().await?;

        for quorum in required_quorums {
            if !confirmed_quorums.contains_key(&(quorum as u32)) {
                return Err(VerificationError::QuorumNotConfirmed);
            }
        }
        Ok(())
    }

    /// Verifies that the certificate is valid
    pub async fn verify_inclusion_data_against_settlement_layer(
        &self,
        cert: &BlobInfo,
    ) -> Result<(), VerificationError> {
        self.verify_batch(cert).await?;
        self.verify_merkle_proof(cert)?;
        self.verify_security_params(cert).await?;
        Ok(())
    }
}
