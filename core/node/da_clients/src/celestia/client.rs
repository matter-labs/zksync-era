use std::{
    fmt::{Debug, Formatter},
    str::FromStr,
    sync::Arc,
    time,
};

use async_trait::async_trait;
use celestia_types::{blob::Commitment, nmt::Namespace, AppVersion, Blob, Height};
use subxt_signer::ExposeSecret;
use tonic::transport::Endpoint;
use zksync_basic_types::ethabi::decode;
use zksync_basic_types::ethabi::{Contract, Event, ParamType, RawTopicFilter};
use zksync_basic_types::web3::{BlockNumber, Filter, FilterBuilder, Log};
use zksync_basic_types::{H256, U256};
use zksync_config::configs::da_client::celestia::{CelestiaConfig, CelestiaSecrets};
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};
use zksync_eth_client::clients::{DynClient, L1};

use crate::{
    celestia::sdk::{BlobTxHash, RawCelestiaClient},
    utils::{to_non_retriable_da_error, to_retriable_da_error},
};

use eq_sdk::{
    get_keccak_inclusion_response::{
        ResponseValue as InclusionResponseValue, Status as InclusionResponseStatus,
    },
    BlobId, EqClient,
};

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Celestia network.
#[derive(Clone)]
pub struct CelestiaClient {
    config: CelestiaConfig,
    eq_client: Arc<EqClient>,
    celestia_client: Arc<RawCelestiaClient>,
    eth_client: Box<DynClient<L1>>,
}

impl CelestiaClient {
    pub async fn new(
        config: CelestiaConfig,
        secrets: CelestiaSecrets,
        eth_client: Box<DynClient<L1>>,
    ) -> anyhow::Result<Self> {
        let celestia_grpc_channel = Endpoint::from_str(config.api_node_url.clone().as_str())?
            .timeout(time::Duration::from_millis(config.timeout_ms))
            .connect()
            .await?;

        let private_key = secrets.private_key.0.expose_secret().to_string();
        let client =
            RawCelestiaClient::new(celestia_grpc_channel, private_key, config.chain_id.clone())
                .expect("could not create Celestia client");

        let eq_service_grpc_channel =
            Endpoint::from_str(config.eq_service_url.clone().as_str())?
                .timeout(time::Duration::from_millis(config.timeout_ms))
                .connect()
                .await?;
        let eq_client = EqClient::new(eq_service_grpc_channel);
        Ok(Self {
            config,
            celestia_client: Arc::new(client),
            eq_client: Arc::new(eq_client),
            eth_client,
        })
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

        if let Err(tonic_status) = self.eq_client.get_keccak_inclusion(&blob_id).await {
            // gRPC error, should be retriable, could be something on the eq-service side
            return Err(DAError {
                error: tonic_status.into(),
                is_retriable: true,
            });
        }

        Ok(DispatchResponse {
            blob_id: blob_id.to_string(),
        })
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        let blob_id_struct = blob_id.parse::<BlobId>()
        .map_err(to_non_retriable_da_error)?;

        let response = self
            .eq_client
            .get_keccak_inclusion(&blob_id_struct)
            .await
            .map_err(to_retriable_da_error)?;
        let response_data: Option<InclusionResponseValue> = response
            .response_value
            .try_into()
            .map_err(to_non_retriable_da_error)?;
        let response_status: InclusionResponseStatus = response
            .status
            .try_into()
            .map_err(to_non_retriable_da_error)?;

        let proof_data = match response_status {
            InclusionResponseStatus::ZkpFinished => match response_data {
                Some(InclusionResponseValue::Proof(proof)) => proof,
                _ => {
                    return Err(DAError { error: anyhow::anyhow!("Complete status should be accompanied by a Proof, eq-service is broken"), is_retriable: false });
                }
            },
            _ => {
                return Ok(None);
            }
        };
        // Here we want to poll blobstream until the included block is in blobstream
        //self.eth_client.call_contract_function(request, block)

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
