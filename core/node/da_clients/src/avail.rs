use std::fmt::Debug;

use async_trait::async_trait;
use jsonrpsee::{
    client_transport::ws::{Url, WsTransportClientBuilder},
    core::client::{Client, ClientBuilder, ClientT, Subscription, SubscriptionClientT},
    rpc_params, tokio,
};
use parity_scale_codec::{Compact, Decode, Encode};
use scale_encode::EncodeAsFields;
use subxt_signer::{
    bip39::Mnemonic,
    sr25519::{Keypair, Signature},
};
use zksync_config::AvailConfig;
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};

const PROTOCOL_VERSION: u8 = 4;

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Avail network.
#[derive(Debug)]
pub struct AvailClient {
    config: AvailConfig,
    client: Client,
    keypair: Keypair,
}

// Utility type needed for encoding the call data
#[derive(parity_scale_codec::Encode, scale_encode::EncodeAsType)]
#[encode_as_type(crate_path = "scale_encode")]
pub struct SubmitData {
    pub data: BoundedVec<u8>,
}

// Utility type needed for encoding the call data
#[derive(parity_scale_codec::Encode, scale_encode::EncodeAsType)]
#[encode_as_type(crate_path = "scale_encode")]
pub struct BoundedVec<_0>(pub Vec<_0>);

#[async_trait]
impl DataAvailabilityClient for AvailClient {
    async fn dispatch_blob(
        &self,
        _: u32, // batch_number
        data: Vec<u8>,
    ) -> anyhow::Result<DispatchResponse, DAError> {
        let call_data = self
            .get_encoded_call(data)
            .await
            .map_err(to_non_retriable_da_error)?;
        let extra_params = self
            .get_extended_params()
            .await
            .map_err(to_non_retriable_da_error)?;
        let additional_params = self
            .get_additional_params()
            .await
            .map_err(to_non_retriable_da_error)?;

        let signature = self.get_signature(
            call_data.clone(),
            extra_params.clone(),
            additional_params.clone(),
        );

        let ext = self.get_submittable_extrinsic(signature, extra_params, call_data);
        let hex_ext = hex::encode(&ext);

        let block_hash = self
            .submit_extrinsic(hex_ext.as_str())
            .await
            .map_err(to_non_retriable_da_error)?;
        let tx_id = self
            .get_tx_id(block_hash.as_str(), hex_ext.as_str())
            .await
            .map_err(to_non_retriable_da_error)?;

        Ok(DispatchResponse {
            blob_id: format!("{}:{}", block_hash, tx_id),
        })
    }

    async fn get_inclusion_data(
        &self,
        _blob_id: &str,
    ) -> anyhow::Result<Option<InclusionData>, DAError> {
        // TODO: implement inclusion data retrieval
        Ok(Some(InclusionData { data: vec![] }))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        let client = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(build_async_client(self.config.api_node_url.as_str()))
            .expect("Failed to build async client");

        Box::new(AvailClient {
            config: self.config.clone(),
            client,
            keypair: self.keypair.clone(),
        })
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(AvailClient::MAX_BLOB_SIZE)
    }
}

impl AvailClient {
    const MAX_BLOB_SIZE: usize = 512 * 1024; // 512kb

    pub async fn new(config: AvailConfig) -> anyhow::Result<Self> {
        let mnemonic = Mnemonic::parse(config.seed.clone())?;
        let keypair = Keypair::from_phrase(&mnemonic, None)?;

        let client = build_async_client(config.api_node_url.as_str()).await?;

        Ok(Self {
            config,
            client,
            keypair,
        })
    }

    async fn get_encoded_call(&self, data: Vec<u8>) -> anyhow::Result<Vec<u8>> {
        let resp: serde_json::Value = self
            .client
            .request("state_getMetadata", rpc_params![])
            .await?;

        let resp = resp
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid metadata"))?
            .to_string();

        let metadata_bytes = hex::decode(
            resp.strip_prefix("0x")
                .ok_or_else(|| anyhow::anyhow!("Metadata doesn't have 0x prefix"))?,
        )?;
        let meta = subxt_metadata::Metadata::decode(&mut &metadata_bytes[..])?;

        let pallet = meta
            .pallet_by_name("DataAvailability")
            .ok_or_else(|| anyhow::anyhow!("DataAvailability pallet not found"))?;

        let call = pallet
            .call_variant_by_name("submit_data")
            .ok_or_else(|| anyhow::anyhow!("submit_data call not found"))?;

        let mut fields = call
            .fields
            .iter()
            .map(|f| scale_encode::Field::new(f.ty.id, f.name.as_deref()));

        let mut bytes = Vec::new();
        pallet.index().encode_to(&mut bytes);
        call.index.encode_to(&mut bytes);

        SubmitData {
            data: BoundedVec(data),
        }
        .encode_as_fields_to(&mut fields, meta.types(), &mut bytes)?;

        Ok(bytes)
    }

    async fn fetch_account_nonce(&self) -> anyhow::Result<u64> {
        let address = to_addr(self.keypair.clone());
        let resp: serde_json::Value = self
            .client
            .request("system_accountNextIndex", rpc_params![address])
            .await?;

        let nonce = resp
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Invalid nonce"))?;

        Ok(nonce)
    }

    // Extrinsic params used here
    // 	CheckMortality<AvailConfig>
    // 	CheckNonce
    // 	ChargeTransactionPayment
    // 	CheckAppId
    async fn get_extended_params(&self) -> anyhow::Result<Vec<u8>> {
        let era = 0u8; // immortal era
        let tip = 0u128; // no tip
        let nonce = self.fetch_account_nonce().await?;

        // Encode the params
        let mut bytes = vec![era];
        Compact(nonce).encode_to(&mut bytes);
        Compact(tip).encode_to(&mut bytes);
        Compact(self.config.app_id).encode_to(&mut bytes);

        Ok(bytes)
    }

    // Extrinsic params used here
    // 	CheckSpecVersion
    // 	CheckTxVersion
    // 	CheckGenesis<AvailConfig>
    async fn get_additional_params(&self) -> anyhow::Result<Vec<u8>> {
        let (spec_version, tx_version) = self.get_runtime_version().await?;
        let genesis_hash = self.fetch_genesis_hash().await?;

        let mut bytes = Vec::new();
        spec_version.encode_to(&mut bytes);
        tx_version.encode_to(&mut bytes);
        // adding genesis hash twice (that's what API requires ¯\_(ツ)_/¯)
        bytes.extend(hex::decode(&genesis_hash)?);
        bytes.extend(hex::decode(&genesis_hash)?);

        Ok(bytes)
    }

    // Fetch the runtime versions
    async fn get_runtime_version(&self) -> anyhow::Result<(u32, u32)> {
        let resp: serde_json::Value = self
            .client
            .request("chain_getRuntimeVersion", rpc_params![])
            .await?;

        let sv = resp
            .get("specVersion")
            .ok_or_else(|| anyhow::anyhow!("Invalid runtime version"))?;
        let tv = resp
            .get("transactionVersion")
            .ok_or_else(|| anyhow::anyhow!("Invalid runtime version"))?;

        let spec_version = sv
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Invalid spec version"))?;
        let transaction_version = tv
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Invalid transaction version"))?;

        Ok((spec_version as u32, transaction_version as u32))
    }

    async fn fetch_genesis_hash(&self) -> anyhow::Result<String> {
        let resp: serde_json::Value = self
            .client
            .request("chain_getBlockHash", rpc_params![0])
            .await?;

        let genesis_hash = resp
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid genesis hash"))?;

        Ok(genesis_hash
            .strip_prefix("0x")
            .ok_or_else(|| anyhow::anyhow!("Genesis hash doesn't have a 0x prefix"))?
            .to_string())
    }

    fn get_signature(
        &self,
        call_data: Vec<u8>,
        extra_params: Vec<u8>,
        additional_params: Vec<u8>,
    ) -> Signature {
        let mut bytes = vec![];
        bytes.extend(call_data);
        bytes.extend(extra_params);
        bytes.extend(additional_params);

        if bytes.len() > 256 {
            bytes = blake2::<32>(bytes).to_vec();
        }

        self.keypair.sign(&bytes)
    }

    fn get_submittable_extrinsic(
        &self,
        signature: Signature,
        extra_params: Vec<u8>,
        call_data: Vec<u8>,
    ) -> Vec<u8> {
        let mut encoded_inner = Vec::new();
        (0b10000000 + PROTOCOL_VERSION).encode_to(&mut encoded_inner); // "is signed" + transaction protocol version

        // sender
        encoded_inner.push(0); // 0 as an id param in MultiAddress enum
        self.keypair.public_key().0.encode_to(&mut encoded_inner); // from address for signature

        // signature
        encoded_inner.push(1); // 1 as an Sr25519 in MultiSignature enum
        signature.0.encode_to(&mut encoded_inner);

        // extra params
        encoded_inner.extend(extra_params);

        // call data
        encoded_inner.extend(call_data);

        // now, prefix with byte length:
        let len = Compact(
            u32::try_from(encoded_inner.len()).expect("extrinsic size expected to be <4GB"),
        );
        let mut encoded = Vec::new();
        len.encode_to(&mut encoded);
        encoded.extend(encoded_inner);

        encoded
    }

    async fn submit_extrinsic(&self, extrinsic: &str) -> anyhow::Result<String> {
        let mut sub: Subscription<serde_json::Value> = self
            .client
            .subscribe(
                "author_submitAndWatchExtrinsic",
                rpc_params![extrinsic],
                "author_unwatchExtrinsic",
            )
            .await?;

        let block_hash = loop {
            let status = sub.next().await.transpose()?;

            if status.is_some() && status.as_ref().unwrap().is_object() {
                if let Some(block_hash) = status.unwrap().get("inBlock") {
                    break block_hash
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Invalid block hash"))?
                        .strip_prefix("0x")
                        .ok_or_else(|| anyhow::anyhow!("Block hash doesn't have 0x prefix"))?
                        .to_string();
                }
            }
        };
        sub.unsubscribe().await?;

        Ok(block_hash)
    }

    async fn get_tx_id(&self, block_hash: &str, hex_ext: &str) -> anyhow::Result<usize> {
        let resp: serde_json::Value = self
            .client
            .request("chain_getBlock", rpc_params![block_hash])
            .await?;

        let block = resp
            .get("block")
            .ok_or_else(|| anyhow::anyhow!("Invalid block"))?;
        let extrinsics = block
            .get("extrinsics")
            .ok_or_else(|| anyhow::anyhow!("No field named extrinsics in block"))?
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Extrinsics field is not an array"))?;

        let hex_ext = format!("0x{}", hex_ext);

        let tx_id = extrinsics
            .iter()
            .position(|extrinsic| extrinsic.as_str() == Some(hex_ext.as_str()))
            .ok_or_else(|| anyhow::anyhow!("Extrinsic not found in block"))?;

        Ok(tx_id)
    }
}

async fn build_async_client(url: &str) -> anyhow::Result<Client> {
    let url = Url::parse(url)?;
    let (tx, rx) = WsTransportClientBuilder::default().build(url).await?;
    let client = ClientBuilder::default().build_with_tokio(tx, rx);

    Ok(client)
}

fn blake2<const N: usize>(data: Vec<u8>) -> [u8; N] {
    blake2b_simd::Params::new()
        .hash_length(N)
        .hash(data.as_slice())
        .as_bytes()
        .try_into()
        .expect("slice is always the necessary length")
}

// Taken from subxt accountId implementation
fn to_addr(keypair: Keypair) -> String {
    // For serializing to a string to obtain the account nonce, we use the default substrate
    // prefix (since we have no way to otherwise pick one). It doesn't really matter, since when
    // it's deserialized back in system_accountNextIndex, we ignore this (so long as it's valid).
    const SUBSTRATE_SS58_PREFIX: u8 = 42;
    // prefix <= 63 just take up one byte at the start:
    let mut v = vec![SUBSTRATE_SS58_PREFIX];
    // then push the account ID bytes.
    v.extend(keypair.public_key().0);
    // then push a 2 byte checksum of what we have so far.
    let r = ss58hash(&v);
    v.extend(&r[0..2]);
    // then encode to base58.
    use base58::ToBase58;
    v.to_base58()
}

// Taken from subxt accountId implementation
fn ss58hash(data: &[u8]) -> Vec<u8> {
    use blake2::{Blake2b512, Digest};
    const PREFIX: &[u8] = b"SS58PRE";
    let mut ctx = Blake2b512::new();
    ctx.update(PREFIX);
    ctx.update(data);
    ctx.finalize().to_vec()
}

pub fn to_non_retriable_da_error(error: impl Into<anyhow::Error>) -> DAError {
    DAError {
        error: error.into(),
        is_retriable: false,
    }
}
