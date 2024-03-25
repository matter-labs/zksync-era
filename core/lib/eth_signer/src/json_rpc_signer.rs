use jsonrpc_core::types::response::Output;
use serde_json::Value;
use zksync_types::{
    tx::primitives::PackedEthSignature, Address, EIP712TypedStructure, Eip712Domain, H256,
};

use crate::{
    error::{RpcSignerError, SignerError},
    json_rpc_signer::messages::JsonRpcRequest,
    raw_ethereum_tx::TransactionParameters,
    EthereumSigner,
};

pub fn is_signature_from_address(
    signature: &PackedEthSignature,
    signed_bytes: &H256,
    address: Address,
) -> Result<bool, SignerError> {
    let signature_is_correct = signature
        .signature_recover_signer(signed_bytes)
        .map_err(|err| SignerError::RecoverAddress(err.to_string()))?
        == address;
    Ok(signature_is_correct)
}

#[derive(Debug, Clone)]
pub enum AddressOrIndex {
    Address(Address),
    Index(usize),
}

#[derive(Debug, Clone)]
pub struct JsonRpcSigner {
    rpc_addr: String,
    client: reqwest::Client,
    address: Option<Address>,
}

#[async_trait::async_trait]
impl EthereumSigner for JsonRpcSigner {
    /// Signs typed struct using Ethereum private key by EIP-712 signature standard.
    /// Result of this function is the equivalent of RPC calling `eth_signTypedData`.
    async fn sign_typed_data<S: EIP712TypedStructure + Sync>(
        &self,
        eip712_domain: &Eip712Domain,
        typed_struct: &S,
    ) -> Result<PackedEthSignature, SignerError> {
        let signature: PackedEthSignature = {
            let message =
                JsonRpcRequest::sign_typed_data(self.address()?, eip712_domain, typed_struct);
            let ret = self
                .post(&message)
                .await
                .map_err(|err| SignerError::SigningFailed(err.to_string()))?;
            serde_json::from_value(ret)
                .map_err(|err| SignerError::SigningFailed(err.to_string()))?
        };

        let signed_bytes =
            PackedEthSignature::typed_data_to_signed_bytes(eip712_domain, typed_struct);
        // Checks the correctness of the message signature without a prefix
        if is_signature_from_address(&signature, &signed_bytes, self.address()?)? {
            Ok(signature)
        } else {
            Err(SignerError::SigningFailed(
                "Invalid signature from JsonRpcSigner".to_string(),
            ))
        }
    }

    /// Signs and returns the RLP-encoded transaction.
    async fn sign_transaction(
        &self,
        raw_tx: TransactionParameters,
    ) -> Result<Vec<u8>, SignerError> {
        let msg = JsonRpcRequest::sign_transaction(self.address()?, raw_tx);

        let ret = self
            .post(&msg)
            .await
            .map_err(|err| SignerError::SigningFailed(err.to_string()))?;

        // get Json object and parse it to get raw Transaction
        let json: Value = serde_json::from_value(ret)
            .map_err(|err| SignerError::SigningFailed(err.to_string()))?;

        let raw_tx: Option<&str> = json
            .get("raw")
            .and_then(|value| value.as_str())
            .map(|value| &value["0x".len()..]);

        if let Some(raw_tx) = raw_tx {
            hex::decode(raw_tx).map_err(|err| SignerError::DecodeRawTxFailed(err.to_string()))
        } else {
            Err(SignerError::DefineAddress)
        }
    }

    async fn get_address(&self) -> Result<Address, SignerError> {
        self.address()
    }
}

impl JsonRpcSigner {
    pub async fn new(
        rpc_addr: impl Into<String>,
        address_or_index: Option<AddressOrIndex>,
        password_to_unlock: Option<String>,
    ) -> Result<Self, SignerError> {
        let mut signer = Self {
            rpc_addr: rpc_addr.into(),
            client: reqwest::Client::new(),
            address: None,
        };

        // If the user has not specified either the index or the address,
        // then we will assume that by default the address will be the first one that the server will send
        let address_or_index = match address_or_index {
            Some(address_or_index) => address_or_index,
            None => AddressOrIndex::Index(0),
        };

        // `EthereumSigner` can support many different addresses,
        // we define only the one we need by the index
        // of receiving from the server or by the address itself.
        signer.detect_address(address_or_index).await?;

        if let Some(password) = password_to_unlock {
            signer.unlock(&password).await?;
        }

        Ok(signer)
    }

    /// Get Ethereum address.
    pub fn address(&self) -> Result<Address, SignerError> {
        self.address.ok_or(SignerError::DefineAddress)
    }

    /// Specifies the Ethereum address which sets the address for which all other requests will be processed.
    /// If the address has already been set, then it will all the same change to a new one.
    pub async fn detect_address(
        &mut self,
        address_or_index: AddressOrIndex,
    ) -> Result<Address, SignerError> {
        self.address = match address_or_index {
            AddressOrIndex::Address(address) => Some(address),
            AddressOrIndex::Index(index) => {
                let message = JsonRpcRequest::accounts();
                let ret = self
                    .post(&message)
                    .await
                    .map_err(|err| SignerError::SigningFailed(err.to_string()))?;
                let accounts: Vec<Address> = serde_json::from_value(ret)
                    .map_err(|err| SignerError::SigningFailed(err.to_string()))?;
                accounts.get(index).copied()
            }
        };

        self.address.ok_or(SignerError::DefineAddress)
    }

    /// Unlocks the current account, after that the server can sign messages and transactions.
    pub async fn unlock(&self, password: &str) -> Result<(), SignerError> {
        let message = JsonRpcRequest::unlock_account(self.address()?, password);
        let ret = self
            .post(&message)
            .await
            .map_err(|err| SignerError::UnlockingFailed(err.to_string()))?;

        let res: bool = serde_json::from_value(ret)
            .map_err(|err| SignerError::UnlockingFailed(err.to_string()))?;

        if res {
            Ok(())
        } else {
            Err(SignerError::UnlockingFailed(
                "Server response: false".to_string(),
            ))
        }
    }

    /// Performs a POST query to the JSON RPC endpoint,
    /// and decodes the response, returning the decoded `serde_json::Value`.
    /// `Ok` is returned only for successful calls, for any kind of error
    /// the `Err` variant is returned (including the failed RPC method
    /// execution response).
    async fn post(
        &self,
        message: impl serde::Serialize,
    ) -> Result<serde_json::Value, RpcSignerError> {
        let reply: Output = self.post_raw(message).await?;

        let ret = match reply {
            Output::Success(success) => success.result,
            Output::Failure(failure) => return Err(RpcSignerError::RpcError(failure)),
        };

        Ok(ret)
    }

    /// Performs a POST query to the JSON RPC endpoint,
    /// and decodes the response, returning the decoded `serde_json::Value`.
    /// `Ok` is returned only for successful calls, for any kind of error
    /// the `Err` variant is returned (including the failed RPC method
    /// execution response).
    async fn post_raw(&self, message: impl serde::Serialize) -> Result<Output, RpcSignerError> {
        let res = self
            .client
            .post(&self.rpc_addr)
            .json(&message)
            .send()
            .await
            .map_err(|err| RpcSignerError::NetworkError(err.to_string()))?;
        if res.status() != reqwest::StatusCode::OK {
            let error = format!(
                "Post query responded with a non-OK response: {}",
                res.status()
            );
            return Err(RpcSignerError::NetworkError(error));
        }
        let reply: Output = res
            .json()
            .await
            .map_err(|err| RpcSignerError::MalformedResponse(err.to_string()))?;

        Ok(reply)
    }
}

mod messages {
    use hex::encode;
    use serde::{Deserialize, Serialize};
    use zksync_types::{
        eip712_signature::utils::get_eip712_json, Address, EIP712TypedStructure, Eip712Domain,
    };

    use crate::raw_ethereum_tx::TransactionParameters;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct JsonRpcRequest {
        pub id: String,
        pub method: String,
        pub jsonrpc: String,
        pub params: Vec<serde_json::Value>,
    }

    impl JsonRpcRequest {
        fn create(method: impl ToString, params: Vec<serde_json::Value>) -> Self {
            Self {
                id: "1".to_owned(),
                jsonrpc: "2.0".to_owned(),
                method: method.to_string(),
                params,
            }
        }

        /// Returns a list of addresses owned by client.
        pub fn accounts() -> Self {
            let params = Vec::new();
            Self::create("eth_accounts", params)
        }

        // Unlocks the address, after that the server can sign messages and transactions.
        pub fn unlock_account(address: Address, password: &str) -> Self {
            let params = vec![
                serde_json::to_value(address).expect("serialization fail"),
                serde_json::to_value(password).expect("serialization fail"),
            ];
            Self::create("personal_unlockAccount", params)
        }

        /// Signs typed struct using Ethereum private key by EIP-712 signature standard.
        /// The address to sign with must be unlocked.
        pub fn sign_typed_data<S: EIP712TypedStructure + Sync>(
            address: Address,
            eip712_domain: &Eip712Domain,
            typed_struct: &S,
        ) -> Self {
            let params = vec![
                serde_json::to_value(address).expect("serialization fail"),
                get_eip712_json(eip712_domain, typed_struct),
            ];

            Self::create("eth_signTypedData_v3", params)
        }

        /// Signs a transaction that can be submitted to the network.
        /// The address to sign with must be unlocked.
        pub fn sign_transaction(from: Address, tx_data: TransactionParameters) -> Self {
            let mut params = Vec::new();

            // Parameter `To` is optional, so we add it only if it is not None
            let tx = if let Some(to) = tx_data.to {
                serde_json::json!({
                    "from": serde_json::to_value(from).expect("serialization fail"),
                    "to": serde_json::to_value(to).expect("serialization fail"),
                    "gas": serde_json::to_value(tx_data.gas).expect("serialization fail"),
                    "gasPrice": serde_json::to_value(tx_data.gas_price).expect("serialization fail"),
                    "maxPriorityFeePerGas": serde_json::to_value(tx_data.max_priority_fee_per_gas).expect("serialization fail"),
                    "maxFeePerGas": serde_json::to_value(tx_data.max_fee_per_gas).expect("serialization fail"),
                    "value": serde_json::to_value(tx_data.value).expect("serialization fail"),
                    "data": serde_json::to_value(format!("0x{}", encode(tx_data.data))).expect("serialization fail"),
                    "nonce": serde_json::to_value(tx_data.nonce).expect("serialization fail"),
                })
            } else {
                serde_json::json!({
                    "from": serde_json::to_value(from).expect("serialization fail"),
                    "gas": serde_json::to_value(tx_data.gas).expect("serialization fail"),
                    "gasPrice": serde_json::to_value(tx_data.gas_price).expect("serialization fail"),
                    "maxPriorityFeePerGas": serde_json::to_value(tx_data.max_priority_fee_per_gas).expect("serialization fail"),
                    "maxFeePerGas": serde_json::to_value(tx_data.max_fee_per_gas).expect("serialization fail"),
                    "value": serde_json::to_value(tx_data.value).expect("serialization fail"),
                    "data": serde_json::to_value(format!("0x{}", encode(tx_data.data))).expect("serialization fail"),
                    "nonce": serde_json::to_value(tx_data.nonce).expect("serialization fail"),
                })
            };
            params.push(tx);
            Self::create("eth_signTransaction", params)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{future::IntoFuture, sync::Arc};

    use axum::{
        extract::{Json, State},
        routing::post,
        Router,
    };
    use futures::future::{AbortHandle, Abortable};
    use jsonrpc_core::{Failure, Id, Output, Success, Version};
    use serde_json::json;
    use zksync_types::{tx::primitives::PackedEthSignature, H256};

    use super::messages::JsonRpcRequest;
    use crate::{raw_ethereum_tx::TransactionParameters, EthereumSigner, JsonRpcSigner};

    async fn index(
        State(state): State<Arc<ServerState>>,
        Json(req): Json<JsonRpcRequest>,
    ) -> Json<serde_json::Value> {
        let resp = match req.method.as_str() {
            "eth_accounts" => {
                let mut addresses = vec![];
                for pk in &state.private_keys {
                    let address = PackedEthSignature::address_from_private_key(pk).unwrap();
                    addresses.push(address)
                }

                create_success(json!(addresses))
            }
            "personal_unlockAccount" => create_success(json!(true)),
            "eth_signTransaction" => {
                let tx_value = json!(req.params[0].clone()).to_string();
                let tx = tx_value.as_bytes();
                let hex_data = hex::encode(tx);
                create_success(json!({ "raw": hex_data }))
            }
            _ => create_fail(req.method.clone()),
        };
        Json(json!(resp))
    }

    fn create_fail(method: String) -> Output {
        Output::Failure(Failure {
            jsonrpc: Some(Version::V2),
            error: jsonrpc_core::Error {
                code: jsonrpc_core::ErrorCode::ParseError,
                message: method,
                data: None,
            },
            id: Id::Num(1),
        })
    }

    fn create_success(result: serde_json::Value) -> Output {
        Output::Success(Success {
            jsonrpc: Some(Version::V2),
            result,
            id: Id::Num(1),
        })
    }
    #[derive(Clone)]
    struct ServerState {
        private_keys: Vec<H256>,
    }

    async fn run_server(state: ServerState) -> (String, AbortHandle) {
        let mut url = None;
        let mut server = None;
        let app = Router::new()
            .route("/", post(index))
            .with_state(Arc::new(state));

        for i in 9000..9999 {
            let new_url = format!("127.0.0.1:{}", i).parse().unwrap();
            if let Ok(axum_server) = axum::Server::try_bind(&new_url) {
                server = Some(axum_server.serve(app.into_make_service()));
                url = Some(new_url);
                break;
            }
        }

        let server = server.expect("Could not bind to port from 9000 to 9999");
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        let future = Abortable::new(server.into_future(), abort_registration);
        tokio::spawn(future);
        let address = format!("http://{}/", &url.unwrap());
        (address, abort_handle)
    }

    #[tokio::test]
    async fn run_client() {
        let (address, abort_handle) = run_server(ServerState {
            private_keys: vec![H256::repeat_byte(0x17)],
        })
        .await;
        // Get address is ok,  unlock address is ok, recover address from signature is also ok
        let client = JsonRpcSigner::new(address, None, None).await.unwrap();

        let transaction_signature = client
            .sign_transaction(TransactionParameters::default())
            .await
            .unwrap();
        assert_ne!(transaction_signature.len(), 0);
        abort_handle.abort();
    }
}
