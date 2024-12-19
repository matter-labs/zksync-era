use std::{collections::HashMap, path::Path};

use ark_bn254::{Fq, G1Affine};
use ethabi::{encode, ParamType, Token};
use rust_kzg_bn254::{blob::Blob, kzg::Kzg, polynomial::PolynomialFormat};
use tokio::{fs::File, io::AsyncWriteExt};
use url::Url;
use zksync_basic_types::web3::CallRequest;
use zksync_eth_client::{clients::PKSigningClient, EnrichedClientResult};
use zksync_types::{
    web3::{self, BlockId, BlockNumber},
    Address, U256, U64,
};

use super::blob_info::{BatchHeader, BlobHeader, BlobInfo, G1Commitment};

#[async_trait::async_trait]
pub trait VerifierClient: Sync + Send + std::fmt::Debug {
    fn clone_boxed(&self) -> Box<dyn VerifierClient>;

    /// Returns the current block number.
    async fn block_number(&self) -> EnrichedClientResult<U64>;

    /// Invokes a function on a contract specified by `contract_address` / `contract_abi` using `eth_call`.
    async fn call_contract_function(
        &self,
        request: web3::CallRequest,
        block: Option<BlockId>,
    ) -> EnrichedClientResult<web3::Bytes>;
}

#[async_trait::async_trait]
impl VerifierClient for PKSigningClient {
    fn clone_boxed(&self) -> Box<dyn VerifierClient> {
        Box::new(self.clone())
    }

    async fn block_number(&self) -> EnrichedClientResult<U64> {
        self.as_ref().block_number().await
    }

    async fn call_contract_function(
        &self,
        request: web3::CallRequest,
        block: Option<BlockId>,
    ) -> EnrichedClientResult<web3::Bytes> {
        self.as_ref().call_contract_function(request, block).await
    }
}

#[derive(Debug)]
pub enum VerificationError {
    ServiceManagerError,
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
    LinkError,
}

/// Configuration for the verifier used for authenticated dispersals
#[derive(Debug, Clone)]
pub struct VerifierConfig {
    pub rpc_url: String,
    pub svc_manager_addr: Address,
    pub max_blob_size: u32,
    pub g1_url: Url,
    pub g2_url: Url,
    pub settlement_layer_confirmation_depth: u32,
    pub private_key: String,
    pub chain_id: u64,
}

/// Verifier used to verify the integrity of the blob info
/// Kzg is used for commitment verification
/// EigenDA service manager is used to connect to the service manager contract
#[derive(Debug)]
pub struct Verifier {
    kzg: Kzg,
    cfg: VerifierConfig,
    signing_client: Box<dyn VerifierClient>,
}

impl Clone for Verifier {
    fn clone(&self) -> Self {
        Self {
            kzg: self.kzg.clone(),
            cfg: self.cfg.clone(),
            signing_client: self.signing_client.clone_boxed(),
        }
    }
}

impl Verifier {
    pub const DEFAULT_PRIORITY_FEE_PER_GAS: u64 = 100;
    pub const SRSORDER: u32 = 268435456; // 2 ^ 28
    pub const G1POINT: &'static str = "g1.point";
    pub const G2POINT: &'static str = "g2.point.powerOf2";
    pub const POINT_SIZE: u32 = 32;

    async fn save_point(url: Url, point: String) -> Result<(), VerificationError> {
        let response = reqwest::get(url)
            .await
            .map_err(|_| VerificationError::LinkError)?;
        if !response.status().is_success() {
            return Err(VerificationError::LinkError);
        }
        let path = format!("./{}", point);
        let path = Path::new(&path);
        let mut file = File::create(path)
            .await
            .map_err(|_| VerificationError::LinkError)?;
        let content = response
            .bytes()
            .await
            .map_err(|_| VerificationError::LinkError)?;
        file.write_all(&content)
            .await
            .map_err(|_| VerificationError::LinkError)?;
        Ok(())
    }
    async fn save_points(url_g1: Url, url_g2: Url) -> Result<String, VerificationError> {
        Self::save_point(url_g1, Self::G1POINT.to_string()).await?;
        Self::save_point(url_g2, Self::G2POINT.to_string()).await?;

        Ok(".".to_string())
    }
    pub async fn new<T: VerifierClient + 'static>(
        cfg: VerifierConfig,
        signing_client: T,
    ) -> Result<Self, VerificationError> {
        let srs_points_to_load = cfg.max_blob_size / Self::POINT_SIZE;
        let path = Self::save_points(cfg.clone().g1_url, cfg.clone().g2_url).await?;
        let kzg_handle = tokio::task::spawn_blocking(move || {
            Kzg::setup(
                &format!("{}/{}", path, Self::G1POINT),
                "",
                &format!("{}/{}", path, Self::G2POINT),
                Self::SRSORDER,
                srs_points_to_load,
                "".to_string(),
            )
        });
        let kzg = kzg_handle
            .await
            .map_err(|e| {
                tracing::error!("Failed to setup KZG: {:?}", e);
                VerificationError::KzgError
            })?
            .map_err(|e| {
                tracing::error!("Failed to setup KZG: {:?}", e);
                VerificationError::KzgError
            })?;

        Ok(Self {
            kzg,
            cfg,
            signing_client: Box::new(signing_client),
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

    pub fn hash_encode_blob_header(&self, blob_header: &BlobHeader) -> Vec<u8> {
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

    pub fn process_inclusion_proof(
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
            let mut buffer = [0u8; 64];
            if index % 2 == 0 {
                buffer[..32].copy_from_slice(&computed_hash);
                buffer[32..].copy_from_slice(&proof[i * 32..(i + 1) * 32]);
            } else {
                buffer[..32].copy_from_slice(&proof[i * 32..(i + 1) * 32]);
                buffer[32..].copy_from_slice(&computed_hash);
            }
            computed_hash = web3::keccak256(&buffer).to_vec();
            index /= 2;
        }

        Ok(computed_hash)
    }

    /// Verifies the certificate's batch root
    pub fn verify_merkle_proof(&self, cert: &BlobInfo) -> Result<(), VerificationError> {
        let inclusion_proof = &cert.blob_verification_proof.inclusion_proof;
        let root = &cert
            .blob_verification_proof
            .batch_medatada
            .batch_header
            .batch_root;
        let blob_index = cert.blob_verification_proof.blob_index;
        let blob_header = &cert.blob_header;

        let blob_header_hash = self.hash_encode_blob_header(blob_header);
        let leaf_hash = web3::keccak256(&blob_header_hash).to_vec();

        let generated_root =
            self.process_inclusion_proof(inclusion_proof, &leaf_hash, blob_index)?;

        if generated_root != *root {
            return Err(VerificationError::DifferentRoots);
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

    /// Retrieves the block to make the request to the service manager
    async fn get_context_block(&self) -> Result<u64, VerificationError> {
        let latest = self
            .signing_client
            .as_ref()
            .block_number()
            .await
            .map_err(|_| VerificationError::ServiceManagerError)?
            .as_u64();

        let depth = self
            .cfg
            .settlement_layer_confirmation_depth
            .saturating_sub(1);
        let block_to_return = latest.saturating_sub(depth as u64);
        Ok(block_to_return)
    }

    async fn call_batch_id_to_metadata_hash(
        &self,
        blob_info: &BlobInfo,
    ) -> Result<Vec<u8>, VerificationError> {
        let context_block = self.get_context_block().await?;

        let func_selector =
            ethabi::short_signature("batchIdToBatchMetadataHash", &[ParamType::Uint(32)]);
        let mut data = func_selector.to_vec();
        let mut batch_id_vec = [0u8; 32];
        U256::from(blob_info.blob_verification_proof.batch_id).to_big_endian(&mut batch_id_vec);
        data.append(batch_id_vec.to_vec().as_mut());

        let call_request = CallRequest {
            to: Some(self.cfg.svc_manager_addr),
            data: Some(zksync_basic_types::web3::Bytes(data)),
            ..Default::default()
        };

        let res = self
            .signing_client
            .as_ref()
            .call_contract_function(
                call_request,
                Some(BlockId::Number(BlockNumber::Number(context_block.into()))),
            )
            .await
            .map_err(|_| VerificationError::ServiceManagerError)?;

        Ok(res.0.to_vec())
    }

    /// Verifies the certificate batch hash
    pub async fn verify_batch(&self, blob_info: &BlobInfo) -> Result<(), VerificationError> {
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
            return Err(VerificationError::DifferentHashes);
        }
        Ok(())
    }

    fn decode_bytes(&self, encoded: Vec<u8>) -> Result<Vec<u8>, VerificationError> {
        let output_type = [ParamType::Bytes];
        let tokens: Vec<Token> = ethabi::decode(&output_type, &encoded)
            .map_err(|_| VerificationError::ServiceManagerError)?;
        let token = tokens
            .first()
            .ok_or(VerificationError::ServiceManagerError)?;
        match token {
            Token::Bytes(data) => Ok(data.to_vec()),
            _ => Err(VerificationError::ServiceManagerError),
        }
    }

    async fn get_quorum_adversary_threshold(
        &self,
        quorum_number: u32,
    ) -> Result<u8, VerificationError> {
        let func_selector = ethabi::short_signature("quorumAdversaryThresholdPercentages", &[]);
        let data = func_selector.to_vec();

        let call_request = CallRequest {
            to: Some(self.cfg.svc_manager_addr),
            data: Some(zksync_basic_types::web3::Bytes(data)),
            ..Default::default()
        };

        let res = self
            .signing_client
            .as_ref()
            .call_contract_function(call_request, None)
            .await
            .map_err(|_| VerificationError::ServiceManagerError)?;

        let percentages = self.decode_bytes(res.0.to_vec())?;

        if percentages.len() > quorum_number as usize {
            return Ok(percentages[quorum_number as usize]);
        }
        Ok(0)
    }

    async fn call_quorum_numbers_required(&self) -> Result<Vec<u8>, VerificationError> {
        let func_selector = ethabi::short_signature("quorumNumbersRequired", &[]);
        let data = func_selector.to_vec();
        let call_request = CallRequest {
            to: Some(self.cfg.svc_manager_addr),
            data: Some(zksync_basic_types::web3::Bytes(data)),
            ..Default::default()
        };

        let res = self
            .signing_client
            .as_ref()
            .call_contract_function(call_request, None)
            .await
            .map_err(|_| VerificationError::ServiceManagerError)?;

        self.decode_bytes(res.0.to_vec())
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
        cert: BlobInfo,
    ) -> Result<(), VerificationError> {
        self.verify_batch(&cert).await?;
        self.verify_merkle_proof(&cert)?;
        self.verify_security_params(&cert).await?;
        Ok(())
    }
}
