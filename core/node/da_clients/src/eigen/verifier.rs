use std::{collections::HashMap, path::Path, sync::Arc};

use ark_bn254::{Fq, G1Affine};
use ethabi::{encode, ParamType, Token};
use rust_kzg_bn254::{blob::Blob, kzg::Kzg, polynomial::PolynomialFormat};
use tokio::{fs::File, io::AsyncWriteExt};
use url::Url;
use zksync_basic_types::web3::CallRequest;
use zksync_eth_client::{clients::PKSigningClient, EnrichedClientError, EnrichedClientResult};
use zksync_types::{
    web3::{self, BlockId, BlockNumber},
    Address, U256, U64,
};

use super::blob_info::{BatchHeader, BlobHeader, BlobInfo, BlobQuorumParam, G1Commitment};

#[async_trait::async_trait]
pub trait VerifierClient: Sync + Send + std::fmt::Debug {
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

#[derive(Debug, thiserror::Error)]
pub enum KzgError {
    #[error("Kzg setup error: {0}")]
    Setup(String),
    #[error(transparent)]
    Internal(#[from] rust_kzg_bn254::errors::KzgError),
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceManagerError {
    #[error(transparent)]
    EnrichedClient(#[from] EnrichedClientError),
    #[error("Decoding error: {0}")]
    Decoding(String),
    #[cfg(test)]
    #[error("Parsing error: {0}")]
    Parsing(String),
}

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error(transparent)]
    ServiceManager(#[from] ServiceManagerError),
    #[error(transparent)]
    Kzg(#[from] KzgError),
    #[error("Wrong proof")]
    WrongProof,
    #[error("Different commitments: expected {expected:?}, got {actual:?}")]
    DifferentCommitments {
        expected: G1Affine,
        actual: G1Affine,
    },
    #[error("Different roots: expected {expected:?}, got {actual:?}")]
    DifferentRoots { expected: String, actual: String },
    #[error("Empty hash")]
    EmptyHash,
    #[error("Different hashes: expected {expected:?}, got {actual:?}")]
    DifferentHashes { expected: String, actual: String },
    #[error("Wrong quorum params: {blob_quorum_params:?}")]
    WrongQuorumParams { blob_quorum_params: BlobQuorumParam },
    #[error("Quorum not confirmed")]
    QuorumNotConfirmed,
    #[error("Commitment not on curve: {0}")]
    CommitmentNotOnCurve(G1Affine),
    #[error("Commitment not on correct subgroup: {0}")]
    CommitmentNotOnCorrectSubgroup(G1Affine),
    #[error("Link Error: {0}")]
    LinkError(String),
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
#[derive(Debug, Clone)]
pub struct Verifier {
    kzg: Kzg,
    cfg: VerifierConfig,
    signing_client: Arc<dyn VerifierClient>,
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
            .map_err(|e| VerificationError::LinkError(e.to_string()))?;
        if !response.status().is_success() {
            return Err(VerificationError::LinkError(
                "Failed to get point".to_string(),
            ));
        }
        let path = format!("./{}", point);
        let path = Path::new(&path);
        let mut file = File::create(path)
            .await
            .map_err(|e| VerificationError::LinkError(e.to_string()))?;
        let content = response
            .bytes()
            .await
            .map_err(|e| VerificationError::LinkError(e.to_string()))?;
        file.write_all(&content)
            .await
            .map_err(|e| VerificationError::LinkError(e.to_string()))?;
        Ok(())
    }

    async fn save_points(url_g1: Url, url_g2: Url) -> Result<String, VerificationError> {
        Self::save_point(url_g1, Self::G1POINT.to_string()).await?;
        Self::save_point(url_g2, Self::G2POINT.to_string()).await?;

        Ok(".".to_string())
    }

    pub(crate) async fn new(
        cfg: VerifierConfig,
        signing_client: Arc<dyn VerifierClient>,
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
            .map_err(|e| VerificationError::Kzg(KzgError::Setup(e.to_string())))?
            .map_err(KzgError::Internal)?;

        Ok(Self {
            kzg,
            cfg,
            signing_client,
        })
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
                expected: expected_commitment,
                actual: actual_commitment,
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

    /// Retrieves the block to make the request to the service manager
    async fn get_context_block(&self) -> Result<u64, VerificationError> {
        let latest = self
            .signing_client
            .as_ref()
            .block_number()
            .await
            .map_err(ServiceManagerError::EnrichedClient)?
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
            .map_err(ServiceManagerError::EnrichedClient)?;

        Ok(res.0.to_vec())
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

    fn decode_bytes(&self, encoded: Vec<u8>) -> Result<Vec<u8>, VerificationError> {
        let output_type = [ParamType::Bytes];
        let tokens: Vec<Token> = ethabi::decode(&output_type, &encoded)
            .map_err(|e| ServiceManagerError::Decoding(e.to_string()))?;
        let token = tokens
            .first()
            .ok_or(ServiceManagerError::Decoding("No tokens found".to_string()))?;
        match token {
            Token::Bytes(data) => Ok(data.to_vec()),
            _ => Err(VerificationError::from(ServiceManagerError::Decoding(
                "Token type mismatch".to_string(),
            ))),
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
            .map_err(ServiceManagerError::EnrichedClient)?;

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
            .map_err(ServiceManagerError::EnrichedClient)?;

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

#[cfg(test)]
mod test {
    use std::{collections::HashMap, str::FromStr, sync::Arc};

    use url::Url;
    use zksync_eth_client::{clients::PKSigningClient, EnrichedClientResult};
    use zksync_types::{
        url::SensitiveUrl,
        web3::{BlockId, Bytes, CallRequest},
        Address, K256PrivateKey, SLChainId, H160, U64,
    };
    use zksync_web3_decl::client::{Client, DynClient, L1};

    use super::ServiceManagerError;
    use crate::eigen::{
        blob_info::{
            BatchHeader, BatchMetadata, BlobHeader, BlobInfo, BlobQuorumParam,
            BlobVerificationProof, G1Commitment,
        },
        verifier::{Verifier, VerifierClient, VerifierConfig},
    };

    fn get_verifier_config() -> VerifierConfig {
        VerifierConfig {
            rpc_url: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            svc_manager_addr: Address::from_str("0xD4A7E1Bd8015057293f0D0A557088c286942e84b").unwrap(),
            max_blob_size: 2 * 1024 * 1024,
            g1_url: Url::parse("https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point").unwrap(),
            g2_url: Url::parse("https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2").unwrap(),
            settlement_layer_confirmation_depth: 0,
            private_key: "0xd08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6"
                .to_string(),
            chain_id: 17000,
        }
    }

    /// Mock struct for the Verifier
    /// Used to avoid making actual calls to a remote disperser
    /// and possible making the CI fail due to network issues.
    /// To run tests with the actual verifier run:
    /// `cargo test -p zksync_da_clients -- --ignored`
    #[derive(Debug)]
    pub struct MockVerifierClient {
        replies: HashMap<String, Bytes>,
    }

    impl MockVerifierClient {
        pub fn new(replies: HashMap<String, Bytes>) -> Self {
            Self { replies }
        }
    }

    #[async_trait::async_trait]
    impl VerifierClient for MockVerifierClient {
        async fn block_number(&self) -> EnrichedClientResult<U64> {
            Ok(U64::from(42))
        }

        async fn call_contract_function(
            &self,
            request: CallRequest,
            _block: Option<BlockId>,
        ) -> EnrichedClientResult<Bytes> {
            let req = serde_json::to_string(&request).unwrap();
            Ok(self.replies.get(&req).unwrap().clone())
        }
    }

    fn create_remote_signing_client(cfg: VerifierConfig) -> PKSigningClient {
        let url = SensitiveUrl::from_str(&cfg.rpc_url).unwrap();
        let query_client: Client<L1> = Client::http(url).unwrap().build();
        let query_client = Box::new(query_client) as Box<DynClient<L1>>;
        PKSigningClient::new_raw(
            K256PrivateKey::from_bytes(
                zksync_types::H256::from_str(&cfg.private_key)
                    .map_err(|e| ServiceManagerError::Parsing(e.to_string()))
                    .unwrap(),
            )
            .map_err(|e| ServiceManagerError::Parsing(e.to_string()))
            .unwrap(),
            cfg.svc_manager_addr,
            Verifier::DEFAULT_PRIORITY_FEE_PER_GAS,
            SLChainId(cfg.chain_id),
            query_client,
        )
    }

    #[ignore = "depends on external RPC"]
    #[tokio::test]
    async fn test_verify_commitment() {
        let cfg = get_verifier_config();
        let signing_client = create_remote_signing_client(cfg.clone());
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
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
        let result = verifier.verify_commitment(commitment, &blob);
        assert!(result.is_ok());
    }

    /// Test the verification of the commitment with a mocked verifier.
    /// To test actual behaviour of the verifier, run the test above
    #[tokio::test]
    async fn test_verify_commitment_mocked() {
        let cfg = get_verifier_config();
        let signing_client = MockVerifierClient::new(HashMap::new());
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
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
        let result = verifier.verify_commitment(commitment, &blob);
        assert!(result.is_ok());
    }

    #[ignore = "depends on external RPC"]
    #[tokio::test]
    async fn test_verify_merkle_proof() {
        let cfg = get_verifier_config();
        let signing_client = create_remote_signing_client(cfg.clone());
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
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
        let result = verifier.verify_merkle_proof(&cert);
        assert!(result.is_ok());
    }

    /// Test the verificarion of a merkle proof with a mocked verifier.
    /// To test actual behaviour of the verifier, run the test above
    #[tokio::test]
    async fn test_verify_merkle_proof_mocked() {
        let cfg = get_verifier_config();
        let signing_client = MockVerifierClient::new(HashMap::new());
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
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
        let result = verifier.verify_merkle_proof(&cert);
        assert!(result.is_ok());
    }

    #[ignore = "depends on external RPC"]
    #[tokio::test]
    async fn test_hash_blob_header() {
        let cfg = get_verifier_config();
        let signing_client = create_remote_signing_client(cfg.clone());
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
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
        let result = verifier.hash_encode_blob_header(&blob_header);
        let expected = "ba4675a31c9bf6b2f7abfdcedd34b74645cb7332b35db39bff00ae8516a67393";
        assert_eq!(result, hex::decode(expected).unwrap());
    }

    /// Test hashing of a blob header with a mocked verifier.
    /// To test actual behaviour of the verifier, run the test above
    #[tokio::test]
    async fn test_hash_blob_header_mocked() {
        let cfg = get_verifier_config();
        let signing_client = MockVerifierClient::new(HashMap::new());
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
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
        let result = verifier.hash_encode_blob_header(&blob_header);
        let expected = "ba4675a31c9bf6b2f7abfdcedd34b74645cb7332b35db39bff00ae8516a67393";
        assert_eq!(result, hex::decode(expected).unwrap());
    }

    #[ignore = "depends on external RPC"]
    #[tokio::test]
    async fn test_inclusion_proof() {
        let cfg = get_verifier_config();
        let signing_client = create_remote_signing_client(cfg.clone());
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
        let proof = hex::decode("c455c1ea0e725d7ea3e5f29e9f48be8fc2787bb0a914d5a86710ba302c166ac4f626d76f67f1055bb960a514fb8923af2078fd84085d712655b58a19612e8cd15c3e4ac1cef57acde3438dbcf63f47c9fefe1221344c4d5c1a4943dd0d1803091ca81a270909dc0e146841441c9bd0e08e69ce6168181a3e4060ffacf3627480bec6abdd8d7bb92b49d33f180c42f49e041752aaded9c403db3a17b85e48a11e9ea9a08763f7f383dab6d25236f1b77c12b4c49c5cdbcbea32554a604e3f1d2f466851cb43fe73617b3d01e665e4c019bf930f92dea7394c25ed6a1e200d051fb0c30a2193c459f1cfef00bf1ba6656510d16725a4d1dc031cb759dbc90bab427b0f60ddc6764681924dda848824605a4f08b7f526fe6bd4572458c94e83fbf2150f2eeb28d3011ec921996dc3e69efa52d5fcf3182b20b56b5857a926aa66605808079b4d52c0c0cfe06923fa92e65eeca2c3e6126108e8c1babf5ac522f4d7").unwrap();
        let leaf: [u8; 32] =
            hex::decode("f6106e6ae4631e68abe0fa898cedbe97dbae6c7efb1b088c5aa2e8b91190ff96")
                .unwrap()
                .try_into()
                .unwrap();
        let expected_root =
            hex::decode("7390b8023db8248123dcaeca57fa6c9340bef639e204f2278fc7ec3d46ad071b")
                .unwrap();

        let actual_root = verifier.process_inclusion_proof(&proof, leaf, 580).unwrap();

        assert_eq!(actual_root, expected_root);
    }

    /// Test proof inclusion with a mocked verifier.
    /// To test actual behaviour of the verifier, run the test above
    #[tokio::test]
    async fn test_inclusion_proof_mocked() {
        let cfg = get_verifier_config();
        let signing_client = MockVerifierClient::new(HashMap::new());
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
        let proof = hex::decode("c455c1ea0e725d7ea3e5f29e9f48be8fc2787bb0a914d5a86710ba302c166ac4f626d76f67f1055bb960a514fb8923af2078fd84085d712655b58a19612e8cd15c3e4ac1cef57acde3438dbcf63f47c9fefe1221344c4d5c1a4943dd0d1803091ca81a270909dc0e146841441c9bd0e08e69ce6168181a3e4060ffacf3627480bec6abdd8d7bb92b49d33f180c42f49e041752aaded9c403db3a17b85e48a11e9ea9a08763f7f383dab6d25236f1b77c12b4c49c5cdbcbea32554a604e3f1d2f466851cb43fe73617b3d01e665e4c019bf930f92dea7394c25ed6a1e200d051fb0c30a2193c459f1cfef00bf1ba6656510d16725a4d1dc031cb759dbc90bab427b0f60ddc6764681924dda848824605a4f08b7f526fe6bd4572458c94e83fbf2150f2eeb28d3011ec921996dc3e69efa52d5fcf3182b20b56b5857a926aa66605808079b4d52c0c0cfe06923fa92e65eeca2c3e6126108e8c1babf5ac522f4d7").unwrap();
        let leaf: [u8; 32] =
            hex::decode("f6106e6ae4631e68abe0fa898cedbe97dbae6c7efb1b088c5aa2e8b91190ff96")
                .unwrap()
                .try_into()
                .unwrap();
        let expected_root =
            hex::decode("7390b8023db8248123dcaeca57fa6c9340bef639e204f2278fc7ec3d46ad071b")
                .unwrap();

        let actual_root = verifier.process_inclusion_proof(&proof, leaf, 580).unwrap();

        assert_eq!(actual_root, expected_root);
    }

    #[ignore = "depends on external RPC"]
    #[tokio::test]
    async fn test_verify_batch() {
        let cfg = get_verifier_config();
        let signing_client = create_remote_signing_client(cfg.clone());
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
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
        let result = verifier.verify_batch(&cert).await;
        assert!(result.is_ok());
    }

    /// Test batch verification with a mocked verifier.
    /// To test actual behaviour of the verifier, run the test above
    #[tokio::test]
    async fn test_verify_batch_mocked() {
        let mut mock_replies = HashMap::new();
        let mock_req = CallRequest {
            from: None,
            to: Some(H160::from_str("0xd4a7e1bd8015057293f0d0a557088c286942e84b").unwrap()),
            gas: None,
            gas_price: None,
            value: None,
            data: Some(Bytes::from(
                hex::decode(
                    "eccbbfc900000000000000000000000000000000000000000000000000000000000103cb",
                )
                .unwrap(),
            )),
            transaction_type: None,
            access_list: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
        };
        let mock_req = serde_json::to_string(&mock_req).unwrap();
        let mock_res = Bytes::from(
            hex::decode("60933e76989e57d6fd210ae2fc3086958d708660ee6927f91963047ab1a91ba8")
                .unwrap(),
        );
        mock_replies.insert(mock_req, mock_res);

        let cfg = get_verifier_config();
        let signing_client = MockVerifierClient::new(mock_replies);
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
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
        let result = verifier.verify_batch(&cert).await;
        assert!(result.is_ok());
    }

    // #[ignore = "depends on external RPC"]
    #[tokio::test]
    async fn test_verify_security_params() {
        let cfg = get_verifier_config();
        let signing_client = create_remote_signing_client(cfg.clone());
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
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
        let result = verifier.verify_security_params(&cert).await;
        assert!(result.is_ok());
    }

    /// Test security params verification with a mocked verifier.
    /// To test actual behaviour of the verifier, run the test above
    #[tokio::test]
    async fn test_verify_security_params_mocked() {
        let mut mock_replies = HashMap::new();

        // First request
        let mock_req = CallRequest {
            from: None,
            to: Some(H160::from_str("0xd4a7e1bd8015057293f0d0a557088c286942e84b").unwrap()),
            gas: None,
            gas_price: None,
            value: None,
            data: Some(Bytes::from(hex::decode("8687feae").unwrap())),
            transaction_type: None,
            access_list: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
        };
        let mock_req = serde_json::to_string(&mock_req).unwrap();
        let mock_res = Bytes::from(
            hex::decode("000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000020001000000000000000000000000000000000000000000000000000000000000")
                .unwrap(),
        );
        mock_replies.insert(mock_req, mock_res);

        // Second request
        let mock_req = CallRequest {
            from: None,
            to: Some(H160::from_str("0xd4a7e1bd8015057293f0d0a557088c286942e84b").unwrap()),
            gas: None,
            gas_price: None,
            value: None,
            data: Some(Bytes::from(hex::decode("e15234ff").unwrap())),
            transaction_type: None,
            access_list: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
        };
        let mock_req = serde_json::to_string(&mock_req).unwrap();
        let mock_res = Bytes::from(
            hex::decode("000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000020001000000000000000000000000000000000000000000000000000000000000")
                .unwrap(),
        );
        mock_replies.insert(mock_req, mock_res);

        let cfg = get_verifier_config();
        let signing_client = MockVerifierClient::new(mock_replies);
        let verifier = Verifier::new(cfg, Arc::new(signing_client)).await.unwrap();
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
        let result = verifier.verify_security_params(&cert).await;
        assert!(result.is_ok());
    }
}
