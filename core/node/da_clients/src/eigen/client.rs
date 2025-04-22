use std::str::FromStr;

use ethabi::{encode, ParamType, Token};
use rust_eigenda_v2_client::{
    core::{BlobKey, Payload, PayloadForm},
    payload_disperser::{PayloadDisperser, PayloadDisperserConfig},
    utils::{PrivateKey, SecretUrl},
};
use subxt_signer::ExposeSecret;
use url::Url;
use zksync_basic_types::web3::CallRequest;
use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};
use zksync_eth_client::EthInterface;
use zksync_types::{Address, H160};
use zksync_web3_decl::client::{Client, DynClient, L1};

use crate::utils::{to_non_retriable_da_error, to_retriable_da_error};

// We can't implement DataAvailabilityClient for an outside struct, so it is needed to defined this intermediate struct
#[derive(Debug, Clone)]
pub struct EigenDAClient {
    client: PayloadDisperser,
    eth_call_client: Box<DynClient<L1>>,
    eigenda_cert_and_blob_verifier_addr: Address,
}

impl EigenDAClient {
    pub async fn new(config: EigenConfig, secrets: EigenSecrets) -> anyhow::Result<Self> {
        let url = Url::from_str(
            config
                .eigenda_eth_rpc
                .clone()
                .ok_or(anyhow::anyhow!("Eigenda eth rpc url is not set"))?
                .expose_str(),
        )
        .map_err(|_| anyhow::anyhow!("Invalid eth rpc url"))?;

        let payload_disperser_config = PayloadDisperserConfig {
            polynomial_form: PayloadForm::Coeff, // todo
            blob_version: 0,                     // todo
            cert_verifier_address: H160([
                0xfe, 0x52, 0xfe, 0x19, 0x40, 0x85, 0x8d, 0xcb, 0x6e, 0x12, 0x15, 0x3e, 0x21, 0x04,
                0xad, 0x0f, 0xdf, 0xbe, 0x11, 0x62,
            ]), // todo
            eth_rpc_url: SecretUrl::new(url),
            disperser_rpc: config.disperser_rpc,
            use_secure_grpc_flag: config.authenticated,
        };

        let private_key = PrivateKey::from_str(secrets.private_key.0.expose_secret())
            .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;
        let client = PayloadDisperser::new(payload_disperser_config, private_key)
            .await
            .map_err(|e| anyhow::anyhow!("Eigen client Error: {:?}", e))?;

        let eth_call_client: Client<L1> = Client::http(
            config
                .eigenda_eth_rpc
                .ok_or(anyhow::anyhow!("Eigenda eth rpc url is not set"))?,
        )
        .map_err(|e| anyhow::anyhow!("Query client Error: {:?}", e))?
        .build();
        let eth_call_client = Box::new(eth_call_client) as Box<DynClient<L1>>;
        Ok(Self {
            client,
            eth_call_client,
            eigenda_cert_and_blob_verifier_addr: config.eigenda_cert_and_blob_verifier_addr,
        })
    }
}

impl EigenDAClient {
    /// This function checks a mapping with form
    /// `mapping(bytes) -> bool` in the EigenDA registry contract.
    /// The name of the mapping is passed as a parameter.
    async fn check_mapping(&self, inclusion_data: &[u8], mapping: &str) -> Result<bool, DAError> {
        let mut data = vec![];
        let func_selector = ethabi::short_signature(mapping, &[ParamType::Bytes]).to_vec();
        data.extend_from_slice(&func_selector);
        let inclusion_data = encode(&[Token::Bytes(inclusion_data.to_owned())]);
        data.extend_from_slice(&inclusion_data);
        let call_request = CallRequest {
            to: Some(self.eigenda_cert_and_blob_verifier_addr),
            data: Some(zksync_basic_types::web3::Bytes(data)),
            ..Default::default()
        };

        let block_id = self
            .eth_call_client
            .block_number()
            .await
            .map_err(to_retriable_da_error)?;
        let res = self
            .eth_call_client
            .as_ref()
            .call_contract_function(call_request, Some(block_id.into()))
            .await
            .map_err(to_retriable_da_error)?;
        match hex::encode(res.0).as_str() {
            "0000000000000000000000000000000000000000000000000000000000000000" => Ok(false),
            "0000000000000000000000000000000000000000000000000000000000000001" => Ok(true),
            _ => Err(anyhow::anyhow!("Invalid response from {}", mapping))
                .map_err(to_non_retriable_da_error),
        }
    }

    async fn check_finished_batches(&self, inclusion_data: &[u8]) -> Result<bool, DAError> {
        self.check_mapping(inclusion_data, "finishedBatches").await
    }

    async fn check_verified_batches(&self, inclusion_data: &[u8]) -> Result<bool, DAError> {
        self.check_mapping(inclusion_data, "verifiedBatches").await
    }

    /// Checks into the EigenDARegistry contract if for the given inclusion data the proof was correctly verified.
    /// It first checks if the proof generation finished, and if it did, it checks if the proof was verified.
    /// If the proof generation didn't finish, it will be called again later when the dispatcher attemps to get inclusion data again
    async fn check_inclusion_data_verification(
        &self,
        inclusion_data: &[u8],
    ) -> Result<Option<bool>, DAError> {
        let finished = self.check_finished_batches(inclusion_data).await?;
        if !finished {
            return Ok(None);
        }
        let verified = self.check_verified_batches(inclusion_data).await?;
        Ok(Some(verified))
    }
}

#[async_trait::async_trait]
impl DataAvailabilityClient for EigenDAClient {
    async fn dispatch_blob(
        &self,
        _: u32, // batch number
        data: Vec<u8>,
    ) -> Result<DispatchResponse, DAError> {
        let payload = Payload::new(data);
        let blob_key = self
            .client
            .send_payload(payload)
            .await
            .map_err(to_retriable_da_error)?;

        Ok(DispatchResponse::from(hex::encode(blob_key.to_bytes())))
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        let bytes = hex::decode(blob_id)
            .map_err(|_| anyhow::anyhow!("Failed to decode blob id: {}", blob_id))
            .map_err(to_non_retriable_da_error)?;
        let blob_key = BlobKey::from_bytes(
            bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Failed to convert bytes to a 32-byte array"))
                .map_err(to_non_retriable_da_error)?,
        );
        let eigen_dacert = self
            .client
            .get_inclusion_data(&blob_key)
            .await
            .map_err(to_retriable_da_error)?;
        if let Some(eigen_dacert) = eigen_dacert {
            let inclusion_data = eigen_dacert.to_bytes(); // todo
            if let Some(verified) = self
                .check_inclusion_data_verification(&inclusion_data)
                .await?
            {
                if verified {
                    Ok(Some(InclusionData {
                        data: inclusion_data,
                    }))
                } else {
                    Err(anyhow::anyhow!("Inclusion data is not verified"))
                        .map_err(to_non_retriable_da_error)
                }
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        PayloadDisperser::blob_size_limit()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Eigen
    }

    async fn balance(&self) -> Result<u64, DAError> {
        Ok(0) // TODO fetch from API when payments are enabled in Eigen (PE-305)
    }
}
