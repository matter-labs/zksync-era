use std::{
    fmt::{Display, Formatter, Result},
    str::FromStr,
    time::{Duration, Instant},
};

use celestia_types::Blob;
use prost::{bytes::Bytes, Message, Name};
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use sha2::Digest;
use tonic::transport::Channel;

use super::{
    celestia_proto::{
        query_client::QueryClient as BlobQueryClient, MsgPayForBlobs,
        QueryParamsRequest as QueryBlobParamsRequest,
    },
    cosmos::{
        auth::{
            query_client::QueryClient as AuthQueryClient, BaseAccount, QueryAccountRequest,
            QueryParamsRequest as QueryAuthParamsRequest,
        },
        bank::v1beta1::{query_client::QueryClient as BankQueryClient, QueryAllBalancesRequest},
        base::{
            node::{
                service_client::ServiceClient as MinGasPriceClient,
                ConfigRequest as MinGasPriceRequest,
            },
            v1beta1::Coin,
        },
        crypto::secp256k1 as ec_proto,
        tx::v1beta1::{
            mode_info::{Single, Sum},
            service_client::ServiceClient as TxClient,
            AuthInfo, BroadcastMode, BroadcastTxRequest, Fee, GetTxRequest, ModeInfo, SignDoc,
            SignerInfo, Tx, TxBody,
        },
    },
    tendermint::types::{Blob as PbBlob, BlobTx},
};

const UNITS_SUFFIX: &str = "utia";
pub const ADDRESS_LENGTH: usize = 20;
const ACCOUNT_ADDRESS_PREFIX: bech32::Hrp = bech32::Hrp::parse_unchecked("celestia");
const BLOB_TX_TYPE_ID: &str = "BLOB";

#[derive(Clone)]
pub(crate) struct RawCelestiaClient {
    grpc_channel: Channel,
    address: String,
    chain_id: String,
    signing_key: SecretKey,
}

impl RawCelestiaClient {
    pub(crate) fn new(
        grpc_channel: Channel,
        private_key: String,
        chain_id: String,
    ) -> anyhow::Result<Self> {
        let signing_key = SecretKey::from_str(&private_key)
            .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;
        let address = get_address(signing_key.public_key(&Secp256k1::new()))?;

        Ok(Self {
            grpc_channel,
            address,
            chain_id,
            signing_key,
        })
    }

    /// Prepares a blob transaction for the given blobs.
    pub(crate) async fn prepare(&self, blobs: Vec<Blob>) -> anyhow::Result<BlobTx> {
        let (gas_per_blob_byte, tx_size_cost_per_byte, min_gas_price, base_account) = tokio::try_join!(
            self.get_gas_per_blob_byte(),
            self.fetch_tx_size_cost_per_byte(),
            self.fetch_min_gas_price(),
            self.fetch_account(),
        )?;

        let msg_pay_for_blobs = new_msg_pay_for_blobs(blobs.as_slice(), self.address.clone())?;

        let gas_limit = estimate_gas(
            &msg_pay_for_blobs.blob_sizes,
            gas_per_blob_byte,
            tx_size_cost_per_byte,
        );
        let fee = calculate_fee(min_gas_price, gas_limit);

        let signed_tx = new_signed_tx(
            &msg_pay_for_blobs,
            &base_account,
            gas_limit,
            fee,
            self.chain_id.clone(),
            &self.signing_key,
        );

        Ok(new_blob_tx(&signed_tx, blobs.iter()))
    }

    /// Submits the blob transaction to the node and returns the height of the block in which it was
    pub(super) async fn submit(
        &self,
        blob_tx_hash: BlobTxHash,
        blob_tx: BlobTx,
    ) -> anyhow::Result<u64> {
        let mut client: TxClient<Channel> = TxClient::new(self.grpc_channel.clone());
        let hex_encoded_tx_hash = self.broadcast_tx(&mut client, blob_tx).await?;
        if hex_encoded_tx_hash != blob_tx_hash.clone().hex() {
            tracing::error!(
                "tx hash {} returned from celestia app is not the same as \
                 the locally calculated one {}; submission file has invalid data",
                hex_encoded_tx_hash,
                blob_tx_hash
            );
        }
        tracing::info!(tx_hash = %hex_encoded_tx_hash, "broadcast blob transaction succeeded");

        let height = self
            .confirm_submission(&mut client, hex_encoded_tx_hash)
            .await;
        Ok(height)
    }

    /// Fetches the gas cost per byte for blobs from the node.
    async fn get_gas_per_blob_byte(&self) -> anyhow::Result<u32> {
        let mut blob_query_client = BlobQueryClient::new(self.grpc_channel.clone());
        let response = blob_query_client.params(QueryBlobParamsRequest {}).await;

        let params = response
            .map_err(|status| {
                anyhow::format_err!(
                    "failed to get blob params, code: {}, message: {}",
                    status.code(),
                    status.message()
                )
            })?
            .into_inner()
            .params
            .ok_or_else(|| anyhow::anyhow!("EmptyBlobParams"))?;

        Ok(params.gas_per_blob_byte)
    }

    /// Fetches the transaction size cost per byte from the node.
    async fn fetch_tx_size_cost_per_byte(&self) -> anyhow::Result<u64> {
        let mut auth_query_client = AuthQueryClient::new(self.grpc_channel.clone());
        let response = auth_query_client.params(QueryAuthParamsRequest {}).await;

        let params = response
            .map_err(|status| {
                anyhow::format_err!(
                    "failed to get auth params, code: {}, message: {}",
                    status.code(),
                    status.message()
                )
            })?
            .into_inner()
            .params
            .ok_or_else(|| anyhow::anyhow!("EmptyAuthParams"))?;

        Ok(params.tx_size_cost_per_byte)
    }

    /// Fetches the minimum gas price from the node.
    async fn fetch_min_gas_price(&self) -> anyhow::Result<f64> {
        let mut min_gas_price_client = MinGasPriceClient::new(self.grpc_channel.clone());
        let response = min_gas_price_client.config(MinGasPriceRequest {}).await;

        let min_gas_price_with_suffix = response
            .map_err(|status| {
                anyhow::format_err!(
                    "failed to get price params, code: {}, message: {}",
                    status.code(),
                    status.message()
                )
            })?
            .into_inner()
            .minimum_gas_price;

        let min_gas_price_str = min_gas_price_with_suffix
            .strip_suffix(UNITS_SUFFIX)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "MinGasPrice bad suffix, min_gas_price: {}, expected_suffix: {}",
                    min_gas_price_with_suffix.clone(),
                    UNITS_SUFFIX
                )
            })?;

        min_gas_price_str.parse::<f64>().map_err(|source| {
            anyhow::anyhow!(
                "Failed to parse min gas price, min_gas_price: {}, err: {}",
                min_gas_price_str,
                source,
            )
        })
    }

    /// Fetches the account info for the current address.
    async fn fetch_account(&self) -> anyhow::Result<BaseAccount> {
        let mut auth_query_client = AuthQueryClient::new(self.grpc_channel.clone());
        let request = QueryAccountRequest {
            address: self.address.clone(),
        };

        let account_info = auth_query_client.account(request).await.map_err(|status| {
            anyhow::anyhow!(
                "failed to get account info, code: {}, message: {}",
                status.code(),
                status.message()
            )
        })?;

        let account_as_any = account_info
            .into_inner()
            .account
            .ok_or_else(|| anyhow::anyhow!("empty account info"))?;
        let expected_type_url = BaseAccount::type_url();

        if expected_type_url == account_as_any.type_url {
            return BaseAccount::decode(&*account_as_any.value)
                .map_err(|error| anyhow::anyhow!("failed to decode account info: {}", error));
        }

        Err(anyhow::anyhow!(
            "unexpected account type, expected: {}, got: {}",
            expected_type_url,
            account_as_any.type_url
        ))
    }

    /// Broadcasts the transaction and returns the transaction hash.
    async fn broadcast_tx(
        &self,
        client: &mut TxClient<Channel>,
        blob_tx: BlobTx,
    ) -> anyhow::Result<String> {
        let request = BroadcastTxRequest {
            tx_bytes: Bytes::from(blob_tx.encode_to_vec()),
            mode: i32::from(BroadcastMode::Sync),
        };

        let mut tx_response = client
            .broadcast_tx(request)
            .await
            .map_err(|status| {
                anyhow::anyhow!(
                    "failed to broadcast the tx, code: {}, message: {}",
                    status.code(),
                    status.message()
                )
            })?
            .into_inner()
            .tx_response
            .ok_or_else(|| anyhow::anyhow!("empty broadcast tx response"))?;

        if tx_response.code != 0 {
            return Err(anyhow::format_err!(
                "failed to broadcast the tx, tx_hash: {}, code: {}, namespace: {}, log: {}",
                tx_response.txhash,
                tx_response.code,
                tx_response.codespace,
                tx_response.raw_log,
            ));
        }

        tx_response.txhash.make_ascii_lowercase();
        Ok(tx_response.txhash)
    }

    /// Waits for the transaction to be included in a block and returns the height of that block.
    async fn confirm_submission(
        &self,
        client: &mut TxClient<Channel>,
        hex_encoded_tx_hash: String,
    ) -> u64 {
        // The min seconds to sleep after receiving a GetTx response and sending the next request.
        const MIN_POLL_INTERVAL_SECS: u64 = 1;
        // The max seconds to sleep after receiving a GetTx response and sending the next request.
        const MAX_POLL_INTERVAL_SECS: u64 = 12;
        // How long to wait after starting `confirm_submission` before starting to log errors.
        const START_LOGGING_DELAY: Duration = Duration::from_secs(12);
        // The minimum duration between logging errors.
        const LOG_ERROR_INTERVAL: Duration = Duration::from_secs(5);

        let start = Instant::now();
        let mut logged_at = start;

        let mut log_if_due = |maybe_error: Option<anyhow::Error>| {
            if start.elapsed() <= START_LOGGING_DELAY || logged_at.elapsed() <= LOG_ERROR_INTERVAL {
                return;
            }
            let reason = maybe_error
                .map_or(anyhow::anyhow!("transaction still pending"), |error| {
                    anyhow::anyhow!("transaction still pending, error: {}", error)
                });
            tracing::warn!(
                %reason,
                tx_hash = %hex_encoded_tx_hash,
                elapsed_seconds = start.elapsed().as_secs_f32(),
                "waiting to confirm blob submission"
            );
            logged_at = Instant::now();
        };

        let mut sleep_secs = MIN_POLL_INTERVAL_SECS;
        loop {
            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
            let res = self
                .clone()
                .get_tx(client, hex_encoded_tx_hash.clone())
                .await;
            match res {
                Ok(Some(height)) => return height,
                Ok(None) => {
                    sleep_secs = MIN_POLL_INTERVAL_SECS;
                    log_if_due(None);
                }
                Err(error) => {
                    sleep_secs =
                        std::cmp::min(sleep_secs.saturating_mul(2), MAX_POLL_INTERVAL_SECS);
                    log_if_due(Some(error));
                }
            }
        }
    }

    /// Returns the height of the block in which the transaction was included (if it was).
    async fn get_tx(
        self,
        client: &mut TxClient<Channel>,
        hex_encoded_tx_hash: String,
    ) -> anyhow::Result<Option<u64>> {
        let request = GetTxRequest {
            hash: hex_encoded_tx_hash,
        };
        let response = client.get_tx(request).await;

        let ok_response = match response {
            Ok(resp) => resp,
            Err(status) => {
                if status.code() == tonic::Code::NotFound {
                    tracing::trace!(msg = status.message(), "transaction still pending");
                    return Ok(None);
                }
                return Err(anyhow::anyhow!(
                    "failed to get tx, code: {}, message: {}",
                    status.code(),
                    status.message()
                ));
            }
        };
        let tx_response = ok_response
            .into_inner()
            .tx_response
            .ok_or_else(|| anyhow::anyhow!("Empty get tx response"))?;
        if tx_response.code != 0 {
            return Err(anyhow::anyhow!(
                "failed to get tx, tx_hash: {}, code: {}, namespace: {}, log: {}",
                tx_response.txhash,
                tx_response.code,
                tx_response.codespace,
                tx_response.raw_log,
            ));
        }
        if tx_response.height == 0 {
            tracing::trace!(tx_hash = %tx_response.txhash, "transaction still pending");
            return Ok(None);
        }

        let height = u64::try_from(tx_response.height).map_err(|_| {
            anyhow::anyhow!("GetTxResponseNegativeBlockHeight: {}", tx_response.height)
        })?;

        tracing::debug!(tx_hash = %tx_response.txhash, height, "transaction succeeded");
        Ok(Some(height))
    }

    pub async fn balance(&self) -> anyhow::Result<u64> {
        let mut auth_query_client = BankQueryClient::new(self.grpc_channel.clone());
        let resp = auth_query_client
            .all_balances(QueryAllBalancesRequest {
                address: self.address.clone(),
                pagination: None,
            })
            .await?;

        let micro_tia_balance = resp
            .into_inner()
            .balances
            .into_iter()
            .find(|coin| coin.denom == UNITS_SUFFIX)
            .map_or_else(
                || {
                    Err(anyhow::anyhow!(
                        "no balance found for address: {}",
                        self.address
                    ))
                },
                |coin| {
                    coin.amount
                        .parse::<u64>()
                        .map_err(|e| anyhow::anyhow!("failed to parse balance: {}", e))
                },
            )?;

        Ok(micro_tia_balance)
    }
}

/// Returns a `BlobTx` for the given signed tx and blobs.
fn new_blob_tx<'a>(signed_tx: &Tx, blobs: impl Iterator<Item = &'a Blob>) -> BlobTx {
    let blobs = blobs
        .map(|blob| PbBlob {
            namespace_id: Bytes::from(blob.namespace.id().to_vec()),
            namespace_version: u32::from(blob.namespace.version()),
            data: Bytes::from(blob.data.clone()),
            share_version: u32::from(blob.share_version),
        })
        .collect();
    BlobTx {
        tx: Bytes::from(signed_tx.encode_to_vec()),
        blobs,
        type_id: BLOB_TX_TYPE_ID.to_string(),
    }
}

/// Returns a signed tx for the given message, account and metadata.
fn new_signed_tx(
    msg_pay_for_blobs: &MsgPayForBlobs,
    base_account: &BaseAccount,
    gas_limit: u64,
    fee: u64,
    chain_id: String,
    signing_key: &SecretKey,
) -> Tx {
    const SIGNING_MODE_INFO: Option<ModeInfo> = Some(ModeInfo {
        sum: Some(Sum::Single(Single { mode: 1 })),
    });

    let fee_coin = Coin {
        denom: UNITS_SUFFIX.to_string(),
        amount: fee.to_string(),
    };
    let fee = Fee {
        amount: vec![fee_coin],
        gas_limit,
        ..Fee::default()
    };

    let public_key = ec_proto::PubKey {
        key: Bytes::from(
            signing_key
                .public_key(&Secp256k1::new())
                .serialize()
                .to_vec(),
        ),
    };
    let public_key_as_any = pbjson_types::Any {
        type_url: ec_proto::PubKey::type_url(),
        value: public_key.encode_to_vec().into(),
    };
    let auth_info = AuthInfo {
        signer_infos: vec![SignerInfo {
            public_key: Some(public_key_as_any),
            mode_info: SIGNING_MODE_INFO,
            sequence: base_account.sequence,
        }],
        fee: Some(fee),
        tip: None,
    };

    let msg = pbjson_types::Any {
        type_url: MsgPayForBlobs::type_url(),
        value: msg_pay_for_blobs.encode_to_vec().into(),
    };
    let tx_body = TxBody {
        messages: vec![msg],
        ..TxBody::default()
    };

    let bytes_to_sign = SignDoc {
        body_bytes: Bytes::from(tx_body.encode_to_vec()),
        auth_info_bytes: Bytes::from(auth_info.encode_to_vec()),
        chain_id,
        account_number: base_account.account_number,
    }
    .encode_to_vec();
    let hashed_bytes: [u8; 32] = sha2::Sha256::digest(bytes_to_sign).into();
    let signature = secp256k1::Secp256k1::new().sign_ecdsa(
        &secp256k1::Message::from_slice(&hashed_bytes[..]).unwrap(), // unwrap is safe here because we know the length of the hashed bytes
        signing_key,
    );
    Tx {
        body: Some(tx_body),
        auth_info: Some(auth_info),
        signatures: vec![Bytes::from(signature.serialize_compact().to_vec())],
    }
}

/// Returns the fee for the signed tx.
fn calculate_fee(min_gas_price: f64, gas_limit: u64) -> u64 {
    let calculated_fee = (min_gas_price * gas_limit as f64).ceil() as u64;
    tracing::info!(
        "calculated fee: {}, min_gas_price: {}, gas_limit: {}",
        calculated_fee,
        min_gas_price,
        gas_limit
    );

    calculated_fee
}

fn estimate_gas(blob_sizes: &[u32], gas_per_blob_byte: u32, tx_size_cost_per_byte: u64) -> u64 {
    // From https://github.com/celestiaorg/celestia-app/blob/v1.4.0/pkg/appconsts/global_consts.go#L28
    const SHARE_SIZE: u64 = 512;
    // From https://github.com/celestiaorg/celestia-app/blob/v1.4.0/pkg/appconsts/global_consts.go#L55
    const CONTINUATION_COMPACT_SHARE_CONTENT_SIZE: u32 = 482;
    // From https://github.com/celestiaorg/celestia-app/blob/v1.4.0/pkg/appconsts/global_consts.go#L59
    const FIRST_SPARSE_SHARE_CONTENT_SIZE: u32 = 478;
    // From https://github.com/celestiaorg/celestia-app/blob/v1.4.0/x/blob/types/payforblob.go#L40
    const PFB_GAS_FIXED_COST: u64 = 75_000;
    // From https://github.com/celestiaorg/celestia-app/blob/v1.4.0/x/blob/types/payforblob.go#L44
    const BYTES_PER_BLOB_INFO: u64 = 70;

    // From https://github.com/celestiaorg/celestia-app/blob/v1.4.0/pkg/shares/share_sequence.go#L126
    //
    // `blob_len` is the size in bytes of one blob's `data` field.
    fn sparse_shares_needed(blob_len: u32) -> u64 {
        if blob_len == 0 {
            return 0;
        }

        if blob_len < FIRST_SPARSE_SHARE_CONTENT_SIZE {
            return 1;
        }

        // Use `u64` here to avoid overflow while adding below.
        let mut bytes_available = u64::from(FIRST_SPARSE_SHARE_CONTENT_SIZE);
        let mut shares_needed = 1_u64;
        while bytes_available < u64::from(blob_len) {
            bytes_available = bytes_available
                .checked_add(u64::from(CONTINUATION_COMPACT_SHARE_CONTENT_SIZE))
                .expect(
                    "this can't overflow, as on each iteration `bytes_available < u32::MAX`, and \
                     we're adding at most `u32::MAX` to it",
                );
            shares_needed = shares_needed.checked_add(1).expect(
                "this can't overflow, as the loop cannot execute for `u64::MAX` iterations",
            );
        }
        shares_needed
    }

    let total_shares_used: u64 = blob_sizes.iter().copied().map(sparse_shares_needed).sum();
    let blob_count = blob_sizes.len().try_into().unwrap_or(u64::MAX);

    let shares_gas = total_shares_used
        .saturating_mul(SHARE_SIZE)
        .saturating_mul(u64::from(gas_per_blob_byte));
    let blob_info_gas = tx_size_cost_per_byte
        .saturating_mul(BYTES_PER_BLOB_INFO)
        .saturating_mul(blob_count);

    shares_gas
        .saturating_add(blob_info_gas)
        .saturating_add(PFB_GAS_FIXED_COST)
}

/// Prepares a `MsgPayForBlobs` message for the given blobs.
fn new_msg_pay_for_blobs(blobs: &[Blob], signer: String) -> anyhow::Result<MsgPayForBlobs> {
    let mut blob_sizes = Vec::with_capacity(blobs.len());
    let mut namespaces = Vec::with_capacity(blobs.len());
    let mut share_commitments = Vec::with_capacity(blobs.len());
    let mut share_versions = Vec::with_capacity(blobs.len());
    for blob in blobs {
        blob_sizes.push(blob.data.len());
        namespaces.push(Bytes::from(blob.namespace.as_bytes().to_vec()));
        share_commitments.push(Bytes::from(blob.commitment.0.to_vec()));
        share_versions.push(u32::from(blob.share_version));
    }

    let blob_sizes = blob_sizes
        .into_iter()
        .map(|blob_size| {
            u32::try_from(blob_size)
                .map_err(|_| anyhow::anyhow!("blob too large, size: {}", blob_size))
        })
        .collect::<anyhow::Result<_, _>>()?;

    Ok(MsgPayForBlobs {
        signer,
        namespaces,
        blob_sizes,
        share_commitments,
        share_versions,
    })
}

fn get_address(public_key: PublicKey) -> anyhow::Result<String> {
    use ripemd::{Digest, Ripemd160};

    let sha_digest = sha2::Sha256::digest(public_key.serialize());
    let ripemd_digest = Ripemd160::digest(&sha_digest[..]);
    let mut bytes = [0u8; ADDRESS_LENGTH];
    bytes.copy_from_slice(&ripemd_digest[..ADDRESS_LENGTH]);

    Ok(bech32::encode::<bech32::Bech32>(
        ACCOUNT_ADDRESS_PREFIX,
        bytes.as_slice(),
    )?)
}

#[derive(Clone, Debug)]
pub(super) struct BlobTxHash([u8; 32]);

impl BlobTxHash {
    pub(super) fn compute(blob_tx: &BlobTx) -> Self {
        Self(sha2::Sha256::digest(&blob_tx.tx).into())
    }

    pub(super) fn hex(self) -> String {
        hex::encode(self.0)
    }
}

impl Display for BlobTxHash {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
        write!(formatter, "{}", hex::encode(self.0))
    }
}
