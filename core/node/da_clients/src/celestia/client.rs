use std::{
    fmt::{Debug, Formatter},
    str::FromStr,
    sync::Arc,
    time,
    fs::File,
};

use async_trait::async_trait;
use celestia_types::{blob::Commitment, nmt::Namespace, AppVersion, Blob, Height};
use subxt_signer::ExposeSecret;
use tonic::transport::Endpoint;
use zksync_basic_types::ethabi::decode;
use zksync_basic_types::{H256, U256};
use zksync_config::configs::da_client::celestia::{CelestiaConfig, CelestiaSecrets};
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};
use zksync_eth_client::{
    EthInterface,
    clients::{DynClient, L1},
};
use zksync_basic_types::web3::{Log, Filter, BlockNumber, FilterBuilder, CallRequest, BlockId};
use zksync_basic_types::ethabi::{Contract, Event, ParamType, RawTopicFilter};

use crate::{
    celestia::sdk::{BlobTxHash, RawCelestiaClient},
    utils::{to_non_retriable_da_error, to_retriable_da_error},
};

use eq_sdk::{EqClient, types::BlobId, get_keccak_inclusion_response::{ResponseValue as InclusionResponseValue, Status as InclusionResponseStatus}};
use crate::celestia::tm_rpc_utils::TendermintRPCClient;
/// An implementation of the `DataAvailabilityClient` trait that interacts with the Celestia network.
#[derive(Clone)]
pub struct CelestiaClient {
    config: CelestiaConfig,
    eq_client: Arc<EqClient>,
    celestia_client: Arc<RawCelestiaClient>,
    eth_client: Box<DynClient<L1>>,
    blobstream_update_event: Event,
    blobstream_contract: Contract,
}

impl CelestiaClient {
    pub async fn new(
        config: CelestiaConfig,
        secrets: CelestiaSecrets,
        eth_client: Box<DynClient<L1>>,
    ) -> anyhow::Result<Self> {
        let contract_file = File::open("blobstream.json")
            .map_err(to_non_retriable_da_error)?;
        let blobstream_contract = Contract::load(contract_file)
            .map_err(to_non_retriable_da_error)?;
        let blobstream_update_event = blobstream_contract.events_by_name("DataCommitmentStored")
            .map_err(to_non_retriable_da_error)?
            .first()
            .ok_or_else(|| to_non_retriable_da_error(anyhow::anyhow!("DataCommitmentStored event not found in contract")))?
            .clone();

        let celestia_grpc_channel = Endpoint::from_str(config.api_node_url.clone().as_str())?
            .timeout(time::Duration::from_millis(config.timeout_ms))
            .connect()
            .await?;

        let private_key = secrets.private_key.0.expose_secret().to_string();
        let client =
            RawCelestiaClient::new(celestia_grpc_channel, private_key, config.chain_id.clone())
                .expect("could not create Celestia client");

        let eq_service_grpc_channel = Endpoint::from_str(config.eq_service_url.clone().as_str())?
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
        let blob_id_struct = blob_id
            .parse::<BlobId>()
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
        let block_num = self.eth_client.block_number()
            .await
            .map_err(|e| to_retriable_da_error(e))?;

        let latest_block = get_latest_block(&self.eth_client, &self.blobstream_contract).await;
        println!("Latest blobstream block: {}", latest_block);

        let target_height: u64 = blob_id_struct.height.into();

        let (from, to) = find_block_range(
            &self.eth_client,
            target_height,
            latest_block,
            BlockNumber::Number(block_num),
            &self.blobstream_update_event,
            &self.blobstream_contract
        ).await.expect("Failed to find block range");

        let tm_rpc_client = TendermintRPCClient::new("http://public-celestia-mocha4-consensus.numia.xyz:26657".to_string());
        let proof = tm_rpc_client.get_data_root_inclusion_proof(target_height, from.as_u64(), to.as_u64()).await.unwrap();
        println!("Proof: {:?}", proof);

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

// The BlobStream contract event
pub struct DataCommitmentStored {
    pub proof_nonce: U256,
    pub start_block: U256,
    pub end_block: U256,
    pub data_commitment: H256,
}

impl DataCommitmentStored {
    pub fn from_log(log: &Log) -> Self {
        DataCommitmentStored {
            proof_nonce: decode(&[ParamType::Uint(256)], &log.data.0)
                .unwrap()[0]
                .clone()
                .into_uint()
                .unwrap(),
            start_block: U256::from_big_endian(&log.topics[1].as_bytes()),
            end_block: U256::from_big_endian(&log.topics[2].as_bytes()),
            data_commitment: H256::from_slice(&log.topics[3].as_bytes()),
        }
    }
}

// Get the latest block relayed to Blobstream
async fn get_latest_block(client: &Box<DynClient<L1>>, contract: &Contract) -> U256 {
    let request = CallRequest {
        to: Some("0xF0c6429ebAB2e7DC6e05DaFB61128bE21f13cb1e".parse().unwrap()),
        data: Some(contract.function("latestBlock").unwrap().encode_input(&[]).unwrap().into()),
        ..Default::default()
    };
    let block_num = client.block_number().await.expect("Could not get block number");
    let result = client.call_contract_function(request, Some(BlockId::Number(block_num.into()))).await.unwrap().0;
    decode(&[ParamType::Uint(256)], &result).unwrap()[0].clone().into_uint().unwrap()
}

// Search for the BlobStream update event that includes the target height
async fn find_block_range(
    client: &Box<DynClient<L1>>,
    target_height: u64,
    latest_block: U256,
    eth_block_num: BlockNumber,
    blobstream_update_event: &Event,
    contract: &Contract,
) -> Result<(U256, U256), Box<dyn std::error::Error>> {
    if target_height < latest_block.as_u64() {
        // Search historical events
        println!("Target height is less than latest block, searching historical events");
        let mut page_start = match eth_block_num {
            BlockNumber::Number(num) => num,
            _ => return Err("Invalid block number".into()),
        };

        let contract_address = "0xF0c6429ebAB2e7DC6e05DaFB61128bE21f13cb1e".parse()?;

        for multiplier in 1.. {  // Infinite iterator with safety check
            if multiplier > 1000 {  // Safety limit to prevent infinite loops
                return Err("Exceeded maximum search depth".into());
            }

            let page_end = page_start - 500 * multiplier;
            let filter = FilterBuilder::default()
                .from_block(BlockNumber::Number(page_end))
                .to_block(BlockNumber::Number(page_start))
                .address(vec![contract_address])
                .topics(Some(vec![blobstream_update_event.signature()]), None, None, None)
                .build();

            let logs = client.logs(&filter).await?;
            
            if let Some(log) = logs.iter().find(|log| {
                let commitment = DataCommitmentStored::from_log(log);
                commitment.start_block.as_u64() <= target_height && 
                commitment.end_block.as_u64() > target_height
            }) {
                let commitment = DataCommitmentStored::from_log(log);
                return Ok((commitment.start_block, commitment.end_block));
            }

            page_start = page_end;
        }
        Err("No matching block range found".into())
    } else {
        // Wait for future blocks
        println!("Target height is greater than latest block, waiting for future updates");
        let mut current_block = latest_block;
        
        while current_block < target_height.into() {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            current_block = get_latest_block(client, &contract).await;
            println!("Latest blobstream block: {}", current_block);
        }
        
        Ok((latest_block, current_block))
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
