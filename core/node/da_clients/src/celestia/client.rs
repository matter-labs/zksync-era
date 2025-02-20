use std::{
    fmt::{Debug, Formatter},
    str::FromStr,
    sync::Arc,
    time,
    fs::File,
};

use async_trait::async_trait;
use celestia_types::{blob::Commitment, nmt::Namespace, Blob};
use eq_common::eqs::{GetKeccakInclusionResponse, get_keccak_inclusion_response::{Status as InclusionResponseStatus, ResponseValue as InclusionResponseValue}};
use serde::{Deserialize, Serialize};
use subxt_signer::ExposeSecret;
use tonic::transport::Endpoint;
use zksync_config::configs::da_client::celestia::{CelestiaConfig, CelestiaSecrets};
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};
use zksync_eth_client::{
    EthInterface,
    clients::{DynClient, L1},
};
use zksync_basic_types::web3::{Log, Filter, BlockNumber, FilterBuilder};
use zksync_basic_types::ethabi::{Contract, Event, ParamType, RawTopicFilter};
use zksync_basic_types::ethabi::decode;
use zksync_basic_types::{U256, H256};

use crate::{
    celestia::sdk::{BlobTxHash, RawCelestiaClient},
    celestia::integration_service::IntegrationClient,
    utils::{to_non_retriable_da_error, to_retriable_da_error},
};

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Avail network.
#[derive(Clone)]
pub struct CelestiaClient {
    config: CelestiaConfig,
    integration_client: Arc<IntegrationClient>,
    celestia_client: Arc<RawCelestiaClient>,
    eth_client: Box<DynClient<L1>>,
    blobstream_update_event: Event,
}

impl CelestiaClient {
    pub async fn new(config: CelestiaConfig, secrets: CelestiaSecrets, eth_client: Box<DynClient<L1>>) -> anyhow::Result<Self> {

        let contract_file = File::open("blobstream.json")
            .map_err(to_non_retriable_da_error)?;
        let contract = Contract::load(contract_file)
            .map_err(to_non_retriable_da_error)?;
        let blobstream_update_event = contract.events_by_name("DataCommitmentStored")
            .map_err(to_non_retriable_da_error)?
            .first()
            .ok_or_else(|| to_non_retriable_da_error(anyhow::anyhow!("DataCommitmentStored event not found in contract")))?
            .clone();

        let celestia_grpc_channel = Endpoint::from_str(config.api_node_url.clone().as_str())?
            .timeout(time::Duration::from_millis(config.timeout_ms))
            .connect()
            .await?;

        let private_key = secrets.private_key.0.expose_secret().to_string();
        let client = RawCelestiaClient::new(celestia_grpc_channel, private_key, config.chain_id.clone())
            .expect("could not create Celestia client");

        let integration_grpc_channel = Endpoint::from_str(config.integration_service_url.clone().as_str())?
            .timeout(time::Duration::from_millis(config.timeout_ms))
            .connect()
            .await?;
        let integration_client = IntegrationClient::new(integration_grpc_channel);
        Ok(Self {
            config,
            celestia_client: Arc::new(client),
            integration_client: Arc::new(integration_client),
            eth_client,
            blobstream_update_event,
        })
    }
}
#[derive(Serialize, Deserialize)]
pub struct BlobId {
    pub commitment: Commitment,
    pub namespace: Namespace,
    pub height: u64,
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
        let blob = Blob::new(namespace, data).map_err(to_non_retriable_da_error)?;

        let commitment = blob.commitment;
        let blob_tx = self
            .celestia_client
            .prepare(vec![blob])
            .await
            .map_err(to_non_retriable_da_error)?;

        let blob_tx_hash = BlobTxHash::compute(&blob_tx);
        let height = self
            .celestia_client
            .submit(blob_tx_hash, blob_tx)
            .await
            .map_err(to_non_retriable_da_error)?;

        let blob_id = BlobId { commitment, namespace, height };
        let blob_bytes = bincode::serialize(&blob_id).map_err(to_non_retriable_da_error)?;

        if let Err(tonic_status) = self.integration_client.get_keccak_inclusion(&blob_id).await {
            // gRPC error, should be retriable, could be something on the eq-service side
            return Err(DAError { error: tonic_status.into(), is_retriable: true });
        }

        Ok(DispatchResponse {
            blob_id: hex::encode(&blob_bytes),
        })
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {

        let blob_id_bytes = hex::decode(blob_id).map_err(to_non_retriable_da_error)?;
        let blob_id: BlobId = bincode::deserialize(&blob_id_bytes).map_err(to_non_retriable_da_error)?;

        let response = self.integration_client.get_keccak_inclusion(&blob_id)
            .await
            .map_err(to_retriable_da_error)?;
        let response_data: Option<InclusionResponseValue> = response.response_value.try_into().map_err(to_non_retriable_da_error)?;
        let response_status: InclusionResponseStatus = response.status.try_into().map_err(to_non_retriable_da_error)?;

        let proof_data = match response_status {
            InclusionResponseStatus::Complete => {
                match response_data {
                    Some(InclusionResponseValue::Proof(proof)) => {
                        proof
                    },
                    _ => {
                        return Err(DAError { error: anyhow::anyhow!("Complete status should be accompanied by a Proof, eq-service is broken"), is_retriable: false });
                    }
                }
            }
            _ => {
                return Ok(None);
            }
        };
        // Here we want to poll blobstream until the included block is in blobstream
        //self.eth_client.call_contract_function(request, block)
        let block_num = self.eth_client.block_number()
            .await
            .map_err(|e| to_retriable_da_error(e))?;


        Ok(Some(InclusionData { data: vec![] }))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(1973786) // almost 2MB
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
