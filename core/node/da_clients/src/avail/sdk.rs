//! Minimal reimplementation of the Avail SDK client required for the DA client implementation.
//! This is considered to be a temporary solution until a mature SDK is available on crates.io

use std::{fmt::Debug, sync::Arc, time};

use backon::{ConstantBuilder, Retryable};
use bytes::Bytes;
use jsonrpsee::{
    core::client::{Client, ClientT, Subscription, SubscriptionClientT},
    rpc_params,
};
use parity_scale_codec::{Compact, Decode, Encode};
use scale_encode::EncodeAsFields;
use serde::{Deserialize, Serialize};
use subxt_signer::{
    bip39::Mnemonic,
    sr25519::{Keypair, Signature},
};
use zksync_types::H256;

use crate::avail::client::to_non_retriable_da_error;

const PROTOCOL_VERSION: u8 = 4;

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Avail network.
#[derive(Debug, Clone)]
pub(crate) struct RawAvailClient {
    app_id: u32,
    keypair: Keypair,
}

/// Utility type needed for encoding the call data
#[derive(parity_scale_codec::Encode, scale_encode::EncodeAsType)]
#[encode_as_type(crate_path = "scale_encode")]
struct SubmitData {
    pub data: BoundedVec<u8>,
}

/// Utility type needed for encoding the call data
#[derive(parity_scale_codec::Encode, scale_encode::EncodeAsType)]
#[encode_as_type(crate_path = "scale_encode")]
struct BoundedVec<_0>(pub Vec<_0>);

impl RawAvailClient {
    pub(crate) const MAX_BLOB_SIZE: usize = 512 * 1024; // 512kb

    pub(crate) async fn new(app_id: u32, seed: &str) -> anyhow::Result<Self> {
        let mnemonic = Mnemonic::parse(seed)?;
        let keypair = Keypair::from_phrase(&mnemonic, None)?;

        Ok(Self { app_id, keypair })
    }

    /// Returns a hex-encoded extrinsic
    pub(crate) async fn build_extrinsic(
        &self,
        client: &Client,
        data: Vec<u8>,
    ) -> anyhow::Result<String> {
        let call_data = self
            .get_encoded_call(client, data)
            .await
            .map_err(to_non_retriable_da_error)?;
        let extra_params = self
            .get_extended_params(client)
            .await
            .map_err(to_non_retriable_da_error)?;
        let additional_params = self
            .get_additional_params(client)
            .await
            .map_err(to_non_retriable_da_error)?;

        let signature = self.get_signature(
            call_data.as_slice(),
            extra_params.as_slice(),
            additional_params.as_slice(),
        );

        let ext = self.get_submittable_extrinsic(
            signature,
            extra_params.as_slice(),
            call_data.as_slice(),
        );

        Ok(hex::encode(&ext))
    }

    /// Returns an encoded call data
    async fn get_encoded_call(
        &self,
        client: &Client,
        data: Vec<u8>,
    ) -> anyhow::Result<Vec<u8>, anyhow::Error> {
        let resp: serde_json::Value = client.request("state_getMetadata", rpc_params![]).await?;

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

    /// Queries a node for a nonce
    async fn fetch_account_nonce(&self, client: &Client) -> anyhow::Result<u64> {
        let address = to_addr(self.keypair.clone());
        let resp: serde_json::Value = client
            .request("system_accountNextIndex", rpc_params![address])
            .await?;

        let nonce = resp
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Invalid nonce"))?;

        Ok(nonce)
    }

    /// Returns a Compact-encoded extended extrinsic parameters
    /// Extrinsic params used here:
    /// - CheckMortality<AvailConfig>
    /// - CheckNonce
    /// - ChargeTransactionPayment
    /// - CheckAppId
    async fn get_extended_params(&self, client: &Client) -> anyhow::Result<Vec<u8>> {
        let era = 0u8; // immortal era
        let tip = 0u128; // no tip
        let nonce = self.fetch_account_nonce(client).await?;

        // Encode the params
        let mut bytes = vec![era];
        Compact(nonce).encode_to(&mut bytes);
        Compact(tip).encode_to(&mut bytes);
        Compact(self.app_id).encode_to(&mut bytes);

        Ok(bytes)
    }

    /// Returns a Compact-encoded additional extrinsic parameters
    /// Extrinsic params used here
    /// - CheckSpecVersion
    /// - CheckTxVersion
    /// - CheckGenesis<AvailConfig>
    async fn get_additional_params(&self, client: &Client) -> anyhow::Result<Vec<u8>> {
        let (spec_version, tx_version) = self.get_runtime_version(client).await?;
        let genesis_hash = self.fetch_genesis_hash(client).await?;

        let mut bytes = Vec::new();
        spec_version.encode_to(&mut bytes);
        tx_version.encode_to(&mut bytes);
        // adding genesis hash twice (that's what API requires ¯\_(ツ)_/¯)
        bytes.extend(hex::decode(&genesis_hash)?);
        bytes.extend(hex::decode(&genesis_hash)?);

        Ok(bytes)
    }

    /// Returns the specification and transaction versions of a runtime
    async fn get_runtime_version(&self, client: &Client) -> anyhow::Result<(u32, u32)> {
        let resp: serde_json::Value = client
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

    async fn fetch_genesis_hash(&self, client: &Client) -> anyhow::Result<String> {
        let resp: serde_json::Value = client.request("chain_getBlockHash", rpc_params![0]).await?;

        let genesis_hash = resp
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid genesis hash"))?;

        Ok(genesis_hash
            .strip_prefix("0x")
            .ok_or_else(|| anyhow::anyhow!("Genesis hash doesn't have a 0x prefix"))?
            .to_string())
    }

    /// Returns a signature for partially-encoded extrinsic
    fn get_signature(
        &self,
        call_data: &[u8],
        extra_params: &[u8],
        additional_params: &[u8],
    ) -> Signature {
        let mut bytes = vec![];
        bytes.extend_from_slice(call_data);
        bytes.extend_from_slice(extra_params);
        bytes.extend_from_slice(additional_params);

        if bytes.len() > 256 {
            bytes = blake2::<32>(bytes).to_vec();
        }

        self.keypair.sign(&bytes)
    }

    /// Encodes all the components of an extrinsic into a single vector
    fn get_submittable_extrinsic(
        &self,
        signature: Signature,
        extra_params: &[u8],
        call_data: &[u8],
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
        encoded_inner.extend_from_slice(extra_params);

        // call data
        encoded_inner.extend_from_slice(call_data);

        // now, prefix with byte length:
        let len = Compact(
            u32::try_from(encoded_inner.len()).expect("extrinsic size expected to be <4GB"),
        );
        let mut encoded = Vec::new();
        len.encode_to(&mut encoded);
        encoded.extend(encoded_inner);

        encoded
    }

    /// Submits an extrinsic. Subscribes to a stream and waits for a tx to be included in a block
    /// to return the block hash
    pub(crate) async fn submit_extrinsic(
        &self,
        client: &Client,
        extrinsic: &str,
    ) -> anyhow::Result<String> {
        let mut sub: Subscription<serde_json::Value> = client
            .subscribe(
                "author_submitAndWatchExtrinsic",
                rpc_params![extrinsic],
                "author_unwatchExtrinsic",
            )
            .await?;

        let block_hash = loop {
            let status = sub.next().await.transpose()?;

            if status.is_some() && status.as_ref().unwrap().is_object() {
                if let Some(block_hash) = status.unwrap().get("finalized") {
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

    /// Iterates over all transaction in the block and finds an ID of the one provided as an argument
    pub(crate) async fn get_tx_id(
        &self,
        client: &Client,
        block_hash: &str,
        hex_ext: &str,
    ) -> anyhow::Result<usize> {
        let resp: serde_json::Value = client
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

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Avail network.
#[derive(Debug, Clone)]
pub(crate) struct GasRelayClient {
    api_url: String,
    api_key: String,
    max_retries: usize,
    api_client: Arc<reqwest::Client>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GasRelayAPISubmissionResponse {
    submission_id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GasRelayAPIStatusResponse {
    submission: GasRelayAPISubmission,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GasRelayAPISubmission {
    block_hash: Option<H256>,
    extrinsic_index: Option<u64>,
}

impl GasRelayClient {
    const DEFAULT_INCLUSION_DELAY: time::Duration = time::Duration::from_secs(60);
    const RETRY_DELAY: time::Duration = time::Duration::from_secs(5);
    pub(crate) async fn new(
        api_url: &str,
        api_key: &str,
        max_retries: usize,
        api_client: Arc<reqwest::Client>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            api_url: api_url.to_owned(),
            api_key: api_key.to_owned(),
            max_retries,
            api_client,
        })
    }

    pub(crate) async fn post_data(&self, data: Vec<u8>) -> anyhow::Result<(H256, u64)> {
        let submit_url = format!("{}/user/submit_raw_data?token=ethereum", &self.api_url);
        // send the data to the gas relay
        let submit_response = self
            .api_client
            .post(&submit_url)
            .body(Bytes::from(data))
            .header("Content-Type", "text/plain")
            .header("Authorization", &self.api_key)
            .send()
            .await?;

        let submit_response = submit_response
            .json::<GasRelayAPISubmissionResponse>()
            .await?;

        let status_url = format!(
            "{}/user/get_submission_info?submission_id={}",
            self.api_url, submit_response.submission_id
        );

        tokio::time::sleep(Self::DEFAULT_INCLUSION_DELAY).await;
        let status_response = (|| async {
            self.api_client
                .get(&status_url)
                .header("Authorization", &self.api_key)
                .send()
                .await
        })
        .retry(
            &ConstantBuilder::default()
                .with_delay(Self::RETRY_DELAY)
                .with_max_times(self.max_retries),
        )
        .await?;

        let status_response = status_response.json::<GasRelayAPIStatusResponse>().await?;
        let (block_hash, extrinsic_index) = (
            status_response.submission.block_hash.ok_or_else(|| {
                anyhow::anyhow!("Block hash not found in the response from the gas relay")
            })?,
            status_response.submission.extrinsic_index.ok_or_else(|| {
                anyhow::anyhow!("Extrinsic index not found in the response from the gas relay")
            })?,
        );

        Ok((block_hash, extrinsic_index))
    }
}
