use std::{
    collections::HashMap,
    fmt::{Debug, Formatter},
    fs::File,
    str::FromStr,
    sync::{Arc, Mutex},
    time,
};

use async_trait::async_trait;
use celestia_types::{nmt::Namespace, AppVersion, Blob, Height};
use eq_sdk::{
    get_keccak_inclusion_response::{
        ResponseValue as InclusionResponseValue, Status as InclusionResponseStatus,
    },
    types::BlobId,
    EqClient,
    KeccakInclusionToDataRootProofOutput,
};
use sp1_sdk::SP1ProofWithPublicValues;
use subxt_signer::ExposeSecret;
use tonic::transport::Endpoint;
use zksync_types::{
    H160,
    ethabi,
    ethabi::{Contract, Event, FixedBytes, Uint, Bytes, Token},
    web3::{BlockNumber, contract::Tokenize},
};
use zksync_config::configs::da_client::celestia::{CelestiaConfig, CelestiaSecrets};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};
use zksync_eth_client::{
    clients::{DynClient, L1},
    EthInterface,
};

use crate::{
    celestia::{
        blobstream::{
            find_block_range, get_latest_blobstream_relayed_height, AttestationProof,
            BinaryMerkleProof, CelestiaZKStackInput, DataRootInclusionProof,
            DataRootInclusionProofResponse, DataRootTuple,
            TendermintRPCClient,
        },
        sdk::{BlobTxHash, RawCelestiaClient},
    },
    utils::{to_non_retriable_da_error, to_retriable_da_error},
};

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Celestia network.
#[derive(Clone)]
pub struct CelestiaClient {
    config: CelestiaConfig,
    eq_client: Arc<EqClient>,
    celestia_client: Arc<RawCelestiaClient>,
    eth_client: Box<DynClient<L1>>,
    blobstream_update_event: Event,
    blobstream_contract: Contract,
    equivalence_proof_cache:
        Arc<Mutex<HashMap<String, (FixedBytes, FixedBytes, SP1ProofWithPublicValues)>>>,
    //blobstream_range_cache: Arc<Mutex<HashMap<u64, (U256, U256, U256)>>>,
}

impl CelestiaClient {
    pub async fn new(
        config: CelestiaConfig,
        secrets: CelestiaSecrets,
        eth_client: Box<DynClient<L1>>,
    ) -> anyhow::Result<Self> {
        let contract_bytes = include_bytes!("blobstream.json");
        let blobstream_contract = Contract::load(contract_bytes.as_ref())
            .map_err(to_non_retriable_da_error)?;
        let blobstream_update_event = blobstream_contract
            .events_by_name("DataCommitmentStored")
            .map_err(to_non_retriable_da_error)?
            .first()
            .ok_or_else(|| {
                to_non_retriable_da_error(anyhow::anyhow!(
                    "DataCommitmentStored event not found in contract"
                ))
            })?
            .clone();

        let celestia_grpc_channel = Endpoint::from_str(config.api_node_url.clone().as_str())?
            .timeout(time::Duration::from_millis(config.timeout_ms))
            .connect()
            .await?;

        let private_key = secrets.private_key.0.expose_secret().to_string();
        let client =
            RawCelestiaClient::new(celestia_grpc_channel, private_key, config.chain_id.clone())
                .expect("could not create Celestia client");

        let eq_service_grpc_channel = Endpoint::from_str(config.eq_service_grpc_url.clone().as_str())?
            .timeout(time::Duration::from_millis(config.timeout_ms))
            .connect()
            .await?;
        let eq_client = EqClient::new(eq_service_grpc_channel);
        Ok(Self {
            config,
            celestia_client: Arc::new(client),
            eq_client: Arc::new(eq_client),
            eth_client,
            blobstream_update_event,
            blobstream_contract,
            equivalence_proof_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    async fn get_proof_data(
        &self,
        blob_id: &str,
    ) -> Result<Option<(FixedBytes, FixedBytes, SP1ProofWithPublicValues)>, DAError> {
        tracing::debug!("Parsing blob id: {}", blob_id);
        let blob_id_struct = blob_id
            .parse::<BlobId>()
            .map_err(to_non_retriable_da_error)?;

        let response = self
            .eq_client
            .get_keccak_inclusion(&blob_id_struct)
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
            _ => {
                tracing::debug!("eq-service returned non-complete status, returning None");
                return Ok(None);
            }
        };
        tracing::debug!("Got proof data from eq-service: {:?}", proof_data);

        let proof: SP1ProofWithPublicValues = bincode::deserialize(&proof_data).unwrap();
        let public_values_bytes = proof.public_values.to_vec();
        //let (keccak_hash, data_root) = 
        let proof_outputs = KeccakInclusionToDataRootProofOutput::from_bytes(&public_values_bytes).map_err(to_non_retriable_da_error)?;
        tracing::debug!(
            "Decoded public values from SP1 proof {:?} {:?}",
            proof_outputs.keccak_hash,
            proof_outputs.data_root
        );

        Ok(Some((FixedBytes::from(proof_outputs.keccak_hash), FixedBytes::from(proof_outputs.data_root), proof)))
    }
}

#[async_trait]
impl DataAvailabilityClient for CelestiaClient {
    async fn dispatch_blob(
        &self,
        _: u32, // batch number
        data: Vec<u8>,
    ) -> Result<DispatchResponse, DAError> {
        let namespace_bytes =
            hex::decode(&self.config.namespace).map_err(to_non_retriable_da_error)?;
        let namespace =
            Namespace::new_v0(namespace_bytes.as_slice()).map_err(to_non_retriable_da_error)?;
        let blob =
            Blob::new(namespace, data, AppVersion::latest()).map_err(to_non_retriable_da_error)?;

        let commitment = blob.commitment;
        let blob_tx = self
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
        .map_err(to_non_retriable_da_error)?;

        let blob_id = BlobId {
            commitment,
            namespace,
            height,
        };

        tracing::debug!("Calling eq-service...");
        if let Err(tonic_status) = self.eq_client.get_keccak_inclusion(&blob_id).await {
            // gRPC error, should be retriable, could be something on the eq-service side
            return Err(DAError {
                error: tonic_status.into(),
                is_retriable: true,
            });
        }
        tracing::debug!("Successfully called eq-service to begin zk equivallence proving");

        Ok(DispatchResponse {
            blob_id: blob_id.to_string(),
        })
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        // First check the cache with a scoped lock
        let cached_proof = {
            let cache = self.equivalence_proof_cache.lock().unwrap();
            cache.get(&blob_id.to_string()).cloned()
        };

        let (keccak_hash, data_root, proof) = match cached_proof {
            Some(cached_proof) => {
                tracing::debug!("Found cached proof for blob_id: {}", blob_id);
                (cached_proof.0, cached_proof.1, cached_proof.2)
            }
            None => {
                tracing::debug!(
                    "Calling get_proof_data and caching result for blob_id: {}",
                    blob_id
                );
                match self.get_proof_data(blob_id).await? {
                    Some(proof) => {
                        tracing::debug!(
                            "Got complete zk equivallence proof for blob_id: {}",
                            blob_id
                        );
                        // Create a new scope for the mutex lock when inserting
                        {
                            let mut cache = self.equivalence_proof_cache.lock().unwrap();
                            cache.insert(blob_id.to_string(), proof.clone());
                        }
                        proof
                    }
                    None => {
                        tracing::debug!(
                            "eq-service is still working on proving blob_id: {}",
                            blob_id
                        );
                        return Ok(None);
                    }
                }
            }
        };

        // Now we begin the blobstream part
        let eth_current_height = self
            .eth_client
            .block_number()
            .await
            .map_err(to_retriable_da_error)?;

        let latest_blobstream_height =
            get_latest_blobstream_relayed_height(&self.eth_client, &self.blobstream_contract, H160::from_str(self.config.blobstream_contract_address.clone().as_str()).map_err(to_non_retriable_da_error)?).await?;
        tracing::debug!("Latest blobstream block: {}", latest_blobstream_height);

        tracing::debug!("Parsing blob id: {}", blob_id);
        let blob_id_struct = blob_id
            .parse::<BlobId>()
            .map_err(to_non_retriable_da_error)?;

        let target_height: u64 = blob_id_struct.height.into();
        tracing::debug!("Checking blobstream for height: {}", target_height);

        let blobstream_contract_address = H160::from_str(self.config.blobstream_contract_address.clone().as_str()).map_err(to_non_retriable_da_error)?;

        // Call find_block_range
        // This function will return None until the relayed height is relayed to blobstream
        let (from, to, proof_nonce) = match find_block_range(
            &self.eth_client,
            target_height,
            latest_blobstream_height,
            BlockNumber::Number(eth_current_height),
            &self.blobstream_update_event,
            blobstream_contract_address,
            self.config.num_pages,
            self.config.page_size,
        )
        .await
        .map_err(|e| to_retriable_da_error(anyhow::anyhow!("Failed to find block range: {}", e)))?
        {
            Some((from, to, proof_nonce)) => {
                tracing::debug!("Found block range: {} - {}", from, to);
                (from, to, proof_nonce)
            }
            None => {
                tracing::debug!("Blobstream is still waiting for height: {}", target_height);
                return Ok(None);
            }
        };

        let tm_rpc_client = TendermintRPCClient::new(self.config.celestia_core_tendermint_rpc_url.clone());
        let data_root_inclusion_proof_string = tm_rpc_client
            .get_data_root_inclusion_proof(target_height, from.as_u64(), to.as_u64())
            .await
            .map_err(|e| DAError {
                error: anyhow::anyhow!("Failed to get data root inclusion proof: {}", e),
                is_retriable: false,
            })?;
        let data_root_inclusion_proof_response: DataRootInclusionProofResponse =
            serde_json::from_str(&data_root_inclusion_proof_string).unwrap();
        let data_root_inclusion_proof: DataRootInclusionProof =
            data_root_inclusion_proof_response.result.proof;

        // data_root_index and total are returned as Strings
        // Parsing into a Uint<256, 4> would be ugly.
        // I think u64 is ok, not sure this will work.
        // Let's find out in testing
        let data_root_index: u64 = data_root_inclusion_proof
            .index
            .parse()
            .map_err(to_non_retriable_da_error)?;

        let evm_index = Uint::from(data_root_index);

        let total: u64 = data_root_inclusion_proof
            .total
            .parse()
            .map_err(to_non_retriable_da_error)?;
        let evm_total = Uint::from(total);

        // Convert proof data into AttestationProof
        let data_root_tuple = DataRootTuple {
            // I think this is correct little endian but we'll see
            height: Uint::from(target_height),
            data_root: data_root,
        };

        let side_nodes: Vec<FixedBytes> = data_root_inclusion_proof.aunts;

        let binary_merkle_proof = BinaryMerkleProof {
            side_nodes: side_nodes,
            key: evm_index,
            num_leaves: evm_total,
        };

        let attestation_proof = AttestationProof {
            tuple_root_nonce: proof_nonce,
            tuple: data_root_tuple,
            proof: binary_merkle_proof,
        };

        let celestia_zkstack_input = CelestiaZKStackInput {
            attestation_proof: attestation_proof,
            equivalence_proof: Bytes::from(proof.bytes()),
            public_values: Bytes::from(proof.public_values.to_vec()),
        };

        Ok(Some(InclusionData {
            data: ethabi::encode(&[Token::Tuple(celestia_zkstack_input.into_tokens())]),
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
        self.celestia_client
            .balance()
            .await
            .map_err(to_non_retriable_da_error)
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
