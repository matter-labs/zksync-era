use std::str::FromStr;

use ethabi::{encode, ParamType, Token};
use rust_eigenda_v2_client::{
    core::BlobKey,
    payload_disperser::{PayloadDisperser, PayloadDisperserConfig},
    rust_eigenda_signers::signers::private_key::Signer,
    utils::SecretUrl,
};
use rust_eigenda_v2_common::{Payload, PayloadForm};
use subxt_signer::ExposeSecret;
use url::Url;
use zksync_basic_types::web3::CallRequest;
use zksync_config::{
    configs::da_client::eigenv2m1::{EigenSecretsV2M1, PolynomialForm},
    EigenConfigV2M1,
};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
    DataAvailabilityClient,
};
use zksync_eth_client::EthInterface;
use zksync_types::Address;
use zksync_web3_decl::client::{Client, DynClient, L1};

use crate::utils::{to_non_retriable_da_error, to_retriable_da_error};

// We can't implement DataAvailabilityClient for an outside struct, so it is needed to defined this intermediate struct
#[derive(Debug, Clone)]
pub struct EigenDAClientV2M1 {
    client: PayloadDisperser,
    eth_call_client: Box<DynClient<L1>>,
    eigenda_cert_and_blob_verifier_addr: Address,
}

impl EigenDAClientV2M1 {
    pub async fn new(config: EigenConfigV2M1, secrets: EigenSecretsV2M1) -> anyhow::Result<Self> {
        let url = Url::from_str(
            config
                .eigenda_eth_rpc
                .clone()
                .ok_or(anyhow::anyhow!("Eigenda eth rpc url is not set"))?
                .expose_str(),
        )
        .map_err(|_| anyhow::anyhow!("Invalid eth rpc url"))?;

        let payload_form = match config.polynomial_form {
            PolynomialForm::Coeff => PayloadForm::Coeff,
            PolynomialForm::Eval => PayloadForm::Eval,
        };

        let payload_disperser_config = PayloadDisperserConfig {
            polynomial_form: payload_form,
            blob_version: config.blob_version,
            cert_verifier_address: config.cert_verifier_addr,
            eth_rpc_url: SecretUrl::new(url),
            disperser_rpc: config.disperser_rpc,
            use_secure_grpc_flag: config.authenticated,
        };

        let private_key = secrets
            .private_key
            .0
            .expose_secret()
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;
        let signer = Signer::new(private_key);
        let client = PayloadDisperser::new(payload_disperser_config, signer)
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

impl EigenDAClientV2M1 {
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
impl DataAvailabilityClient for EigenDAClientV2M1 {
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

        Ok(DispatchResponse::from(blob_key.to_hex()))
    }

    async fn ensure_finality(
        &self,
        dispatch_request_id: String,
    ) -> Result<Option<FinalityResponse>, DAError> {
        // TODO: return a quick confirmation in `dispatch_blob` and await here
        Ok(Some(FinalityResponse {
            blob_id: dispatch_request_id,
        }))
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        let blob_key = BlobKey::from_hex(blob_id)
            .map_err(|_| anyhow::anyhow!("Failed to decode blob id: {}", blob_id))
            .map_err(to_non_retriable_da_error)?;
        let eigenda_cert = self
            .client
            .get_inclusion_data(&blob_key)
            .await
            .map_err(to_retriable_da_error)?;
        if let Some(eigenda_cert) = eigenda_cert {
            let inclusion_data = eigenda_cert
                .to_bytes()
                .map_err(|_| anyhow::anyhow!("Failed to convert eigenda cert to bytes"))
                .map_err(to_non_retriable_da_error)?;
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
        PayloadDisperser::<Signer>::blob_size_limit()
    }

    fn client_type(&self) -> ClientType {
        ClientType::EigenV2M1
    }

    async fn balance(&self) -> Result<u64, DAError> {
        Ok(0) // TODO fetch from API when payments are enabled in Eigen (PE-305)
    }
}
