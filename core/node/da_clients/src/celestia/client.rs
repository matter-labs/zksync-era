use std::{
    fmt::{Debug, Formatter},
    str::FromStr,
    sync::Arc,
};

use async_trait::async_trait;
use celestia_grpc::{GrpcClient, TxConfig};
use celestia_types::{nmt::Namespace, state::Address, AppVersion, Blob, Height};
use chrono::{DateTime, Utc};
use eq_sdk::{
    get_zk_stack_response::{
        ResponseValue as InclusionResponseValue, Status as InclusionResponseStatus,
    },
    types::BlobId,
    types::JobId,
    EqClient,
};
use secp256k1::SecretKey;
use secrecy::ExposeSecret;
use tendermint::crypto::default::ecdsa_secp256k1::SigningKey;
use tonic::transport::Channel;
use tonic::transport::Endpoint;
use zksync_basic_types::L2ChainId;
use zksync_config::configs::da_client::celestia::{CelestiaConfig, CelestiaSecrets};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
    DataAvailabilityClient,
};
use zksync_eth_client::{
    clients::{DynClient, L1},
    EthInterface,
};
use zksync_types::{
    ethabi,
    ethabi::{Bytes, FixedBytes, Token, Uint},
    web3::{contract::Tokenize, BlockNumber},
    H160, U256, U64,
};

use crate::{
    celestia::blobstream::{
        find_block_range, get_latest_blobstream_relayed_height, AttestationProof,
        BinaryMerkleProof, CelestiaZKStackInput, DataRootInclusionProof,
        DataRootInclusionProofResponse, DataRootTuple, TendermintRPCClient,
    },
    utils::{to_non_retriable_da_error, to_retriable_da_error},
};

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Celestia network.
#[derive(Clone)]
pub struct CelestiaClient {
    config: CelestiaConfig,
    verify_inclusion: bool,
    eq_client: Option<Arc<EqClient>>,
    celestia_client: Arc<GrpcClient>,
    eth_client: Box<DynClient<L1>>,
    l2_chain_id: L2ChainId,
    address: Address,
}

impl CelestiaClient {
    pub async fn new(
        config: CelestiaConfig,
        secrets: CelestiaSecrets,
        eth_client: Box<DynClient<L1>>,
        l2_chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        let verify_inclusion = config.inclusion_verification.is_some();

        let eq_client = if verify_inclusion {
            let eq_service_grpc_channel = Endpoint::from_str(
                config
                    .inclusion_verification
                    .as_ref()
                    .unwrap()
                    .eq_service_grpc_url
                    .clone()
                    .as_str(),
            )?
            .timeout(config.timeout)
            .connect()
            .await?;
            let eq_client = EqClient::new(eq_service_grpc_channel);
            Some(Arc::new(eq_client))
        } else {
            None
        };

        let private_key = secrets.private_key.0.expose_secret().to_string();

        let signing_key = SecretKey::from_str(private_key.as_str())?;
        let signing_key_bytes = signing_key.secret_bytes();
        let signing_key_tendermint = SigningKey::from_bytes(&signing_key_bytes.into())?;

        let address = Address::from_account_veryfing_key(*signing_key_tendermint.verifying_key());

        tracing::debug!("creating celestia client");
        let client = GrpcClient::builder()
            .pubkey_and_signer(
                *signing_key_tendermint.verifying_key(),
                signing_key_tendermint,
            )
            .url(config.api_node_url.clone())
            .build()?;

        tracing::debug!("celestia client created");

        Ok(Self {
            verify_inclusion,
            config,
            celestia_client: Arc::new(client),
            eq_client: eq_client,
            eth_client,
            l2_chain_id,
            address,
        })
    }

    async fn get_eq_proof(&self, blob_id: &str) -> Result<Option<(Vec<u8>, Vec<u8>)>, DAError> {
        tracing::debug!("Parsing blob id: {}", blob_id);
        let blob_id_struct = blob_id
            .parse::<BlobId>()
            .map_err(to_non_retriable_da_error)?;
        tracing::debug!("blob_id_struct: {:?}", blob_id_struct);

        let response = self
            .eq_client
            .as_ref()
            .unwrap()
            .get_zk_stack(&JobId::new(blob_id_struct, self.l2_chain_id.as_u64(), 0))
            .await
            .map_err(to_retriable_da_error)?;

        tracing::debug!("Got response from eq-service");
        let response_data: Option<InclusionResponseValue> = response
            .response_value
            .try_into()
            .map_err(to_non_retriable_da_error)?;
        tracing::debug!("response_data: {:?}", response_data);

        let response_status: InclusionResponseStatus = response
            .status
            .try_into()
            .map_err(to_non_retriable_da_error)?;
        tracing::debug!("response_status: {:?}", response_status);

        let proof_data = match response_status {
            InclusionResponseStatus::ZkpFinished => match response_data {
                Some(InclusionResponseValue::Proof(proof)) => proof,
                _ => {
                    return Err(DAError {
                        error: anyhow::anyhow!("Complete status should be accompanied by a Proof, eq-service is broken"),
                        is_retriable: false
                    });
                }
            },
            InclusionResponseStatus::PermanentFailure => {
                return Err(DAError {
                    error: anyhow::anyhow!("eq-service returned PermanentFailure"),
                    is_retriable: false,
                });
            }
            InclusionResponseStatus::RetryableFailure => {
                return Err(DAError {
                    error: anyhow::anyhow!("eq-service returned RetryableFailure"),
                    is_retriable: true,
                });
            }
            _ => {
                tracing::debug!("eq-service returned non-complete status, returning None");
                return Ok(None);
            }
        };
        tracing::debug!("Got proof data from eq-service: {:?}", proof_data);

        let proof = proof_data.proof_data;
        let public_values = proof_data.public_values;
        Ok(Some((public_values, proof)))
    }

    async fn fetch_heights_and_parse_blob_id(
        &self,
        blob_id: &str,
    ) -> Result<(U64, U256, BlobId), DAError> {
        let eth_current_height = self
            .eth_client
            .block_number()
            .await
            .map_err(to_retriable_da_error)?;

        // unwrap should be safe on these config fields because we checked verify_inclusion before get_inclusion_data calls this function
        let blobstream_addr = H160::from_str(
            self.config
                .inclusion_verification
                .as_ref()
                .unwrap()
                .blobstream_contract_address
                .as_str(),
        )
        .map_err(to_non_retriable_da_error)?;

        let latest_blobstream_height =
            get_latest_blobstream_relayed_height(&self.eth_client, blobstream_addr).await?;
        tracing::debug!("Latest blobstream block: {}", latest_blobstream_height);

        tracing::debug!("Parsing blob id: {}", blob_id);
        let blob_id_struct = blob_id
            .parse::<BlobId>()
            .map_err(to_non_retriable_da_error)?;

        Ok((eth_current_height, latest_blobstream_height, blob_id_struct))
    }

    async fn find_blobstream_block_range(
        &self,
        target_height: u64,
        latest_blobstream_height: U256,
        eth_current_height: U64,
    ) -> Result<Option<(U256, U256, U256)>, DAError> {
        // unwrap should be safe on these config fields because we checked verify_inclusion before get_inclusion_data calls this function
        let blobstream_addr = H160::from_str(
            self.config
                .inclusion_verification
                .as_ref()
                .unwrap()
                .blobstream_contract_address
                .as_str(),
        )
        .map_err(to_non_retriable_da_error)?;

        find_block_range(
            &self.eth_client,
            target_height,
            latest_blobstream_height,
            BlockNumber::Number(eth_current_height),
            blobstream_addr,
            self.config
                .inclusion_verification
                .as_ref()
                .unwrap()
                .blobstream_events_num_pages,
            self.config
                .inclusion_verification
                .as_ref()
                .unwrap()
                .blobstream_events_page_size,
        )
        .await
        .map_err(|e| to_retriable_da_error(anyhow::anyhow!("Failed to find block range: {}", e)))
    }

    async fn create_attestation_proof(
        &self,
        target_height: u64,
        from: U256,
        to: U256,
        proof_nonce: U256,
    ) -> Result<AttestationProof, DAError> {
        let tm_rpc_client =
            // unwrap should be safe
            // create_attestation_proof is only called by get_inclusion_data if verify_inclusion is true
            TendermintRPCClient::new(self.config.inclusion_verification.as_ref().unwrap().celestia_core_tendermint_rpc_url.clone());

        // Get data root inclusion proof
        let data_root_inclusion_proof = self
            .fetch_data_root_inclusion_proof(
                &tm_rpc_client,
                target_height,
                from.as_u64(),
                to.as_u64(),
            )
            .await?;

        // Get data root
        let data_root = tm_rpc_client
            .get_data_root(target_height)
            .await
            .map_err(to_retriable_da_error)?;

        // Create attestation proof components
        let data_root_tuple = DataRootTuple {
            height: Uint::from(target_height),
            data_root: data_root.to_vec(),
        };

        let data_root_index = Uint::from_dec_str(&data_root_inclusion_proof.index)
            .map_err(to_non_retriable_da_error)?;

        let total: u64 = data_root_inclusion_proof
            .total
            .parse()
            .map_err(to_non_retriable_da_error)?;
        let evm_total = Uint::from(total);

        let side_nodes: Vec<FixedBytes> = data_root_inclusion_proof.aunts;

        let binary_merkle_proof = BinaryMerkleProof {
            side_nodes,
            key: data_root_index,
            num_leaves: evm_total,
        };

        Ok(AttestationProof {
            tuple_root_nonce: proof_nonce,
            tuple: data_root_tuple,
            proof: binary_merkle_proof,
        })
    }

    async fn fetch_data_root_inclusion_proof(
        &self,
        tm_rpc_client: &TendermintRPCClient,
        target_height: u64,
        from: u64,
        to: u64,
    ) -> Result<DataRootInclusionProof, DAError> {
        let data_root_inclusion_proof_string = tm_rpc_client
            .get_data_root_inclusion_proof(target_height, from, to)
            .await
            .map_err(|e| DAError {
                error: anyhow::anyhow!("Failed to get data root inclusion proof: {}", e),
                is_retriable: false,
            })?;
        tracing::debug!(
            "data_root_inclusion_proof_string: {}",
            data_root_inclusion_proof_string
        );
        let data_root_inclusion_proof_response: DataRootInclusionProofResponse =
            serde_json::from_str(&data_root_inclusion_proof_string).unwrap();

        Ok(data_root_inclusion_proof_response.result.proof)
    }

    fn create_inclusion_data(
        &self,
        attestation_proof: AttestationProof,
        proof_data: (Vec<u8>, Vec<u8>),
    ) -> Result<Option<InclusionData>, DAError> {
        let (public_values, proof) = proof_data;

        let celestia_zkstack_input = CelestiaZKStackInput {
            attestation_proof,
            equivalence_proof: Bytes::from(proof),
            public_values: Bytes::from(public_values),
        };

        Ok(Some(InclusionData {
            data: ethabi::encode(&[Token::Tuple(celestia_zkstack_input.into_tokens())]),
        }))
    }
}

#[async_trait]
impl DataAvailabilityClient for CelestiaClient {
    async fn dispatch_blob(
        &self,
        batch_number: u32, // batch number
        data: Vec<u8>,
    ) -> Result<DispatchResponse, DAError> {
        let namespace_bytes =
            hex::decode(&self.config.namespace).map_err(to_non_retriable_da_error)?;
        let namespace =
            Namespace::new_v0(namespace_bytes.as_slice()).map_err(to_non_retriable_da_error)?;
        let blob = Blob::new(namespace, data, None, AppVersion::latest())
            .map_err(to_non_retriable_da_error)?;

        let commitment = blob.commitment;
        /*let blob_tx = self
            .celestia_client
            .prepare(vec![blob])
            .await
            .map_err(to_non_retriable_da_error)?;

        let blob_tx_hash = BlobTxHash::compute(&blob_tx);
        let height = <u64 as TryInto<Height>>::try_into(
            self.celestia_client
                .submit(blob_tx_hash, blob_tx)
                .await
                .map_err(to_non_retriable_da_error)?,
        )
        .map_err(to_non_retriable_da_error)?;*/
        let tx_info = self
            .celestia_client
            .submit_blobs(&[blob], TxConfig::default())
            .await
            .map_err(to_retriable_da_error)?;
        let height = tx_info.height.into();

        let blob_id = BlobId {
            commitment,
            namespace,
            height,
        };

        Ok(DispatchResponse {
            request_id: blob_id.to_string(),
        })
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        // Step 1: Get heights and validate blob ID
        let (eth_current_height, latest_blobstream_height, blob_id_struct) =
            self.fetch_heights_and_parse_blob_id(blob_id).await?;

        let target_height: u64 = blob_id_struct.height.into();
        tracing::debug!("Checking blobstream for height: {}", target_height);

        // Step 2: Find block range
        let block_range = self
            .find_blobstream_block_range(
                target_height,
                latest_blobstream_height,
                eth_current_height,
            )
            .await?;

        let (from, to, proof_nonce) = match block_range {
            Some(range) => range,
            None => {
                tracing::debug!("Blobstream is still waiting for height: {}", target_height);
                return Ok(None);
            }
        };

        // Step 3: Get proof data
        let attestation_proof = self
            .create_attestation_proof(target_height, from, to, proof_nonce)
            .await?;

        // Step 4: Get proof data for the blob
        let proof_data = match self.get_eq_proof(blob_id).await? {
            Some(p) => p,
            None => return Ok(None),
        };

        tracing::info!("public_values: {:?}", hex::encode(&proof_data.0));
        tracing::info!("proof: {:?}", hex::encode(&proof_data.1));

        // Step 5: Combine everything into inclusion data
        self.create_inclusion_data(attestation_proof, proof_data)
    }

    async fn ensure_finality(
        &self,
        dispatch_request_id: String,
        _: DateTime<Utc>,
    ) -> Result<Option<FinalityResponse>, DAError> {
        if !self.verify_inclusion {
            return Ok(None);
        }

        let blob_id = dispatch_request_id
            .parse::<BlobId>()
            .map_err(to_non_retriable_da_error)?;

        tracing::debug!("Calling eq-service...");
        // unwrap should be safe because we checked that verify_inclusion is true in the beginning
        if let Err(tonic_status) = self
            .eq_client
            .as_ref()
            .unwrap()
            .get_zk_stack(&JobId::new(blob_id, self.l2_chain_id.as_u64(), 0))
            .await
        {
            // gRPC error, should be retriable, could be something on the eq-service side
            return Err(DAError {
                error: tonic_status.into(),
                is_retriable: true,
            });
        }
        tracing::debug!("Successfully called eq-service to begin zk equivallence proving");

        // TODO: return a quick confirmation in `dispatch_blob` and await here
        Ok(Some(FinalityResponse {
            blob_id: dispatch_request_id,
        }))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(1973786) // almost 2MB
    }

    fn client_type(&self) -> ClientType {
        ClientType::Celestia
    }

    async fn balance(&self) -> Result<u64, DAError> {
        Ok(self
            .celestia_client
            .get_balance(&self.address, "utia")
            .await
            .map_err(to_retriable_da_error)?
            .amount())
    }
}

impl Debug for CelestiaClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CelestiaClient")
            .field("config.api_node_url", &self.config.api_node_url)
            .field("config.namespace", &self.config.namespace)
            .finish()
    }
}
