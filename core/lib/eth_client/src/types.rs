use rlp::RlpStream;
use zksync_types::{
    eth_sender::EthTxBlobSidecar,
    web3::{
        contract::{
            tokens::{Detokenize, Tokenize},
            Error as ContractError, Options,
        },
        ethabi,
        types::{Address, BlockId, TransactionReceipt, H256, U256},
    },
};

/// Wrapper for `Vec<ethabi::Token>` that doesn't wrap them in an additional array in `Tokenize` implementation.
#[derive(Debug)]
pub(crate) struct RawTokens(pub Vec<ethabi::Token>);

impl Tokenize for RawTokens {
    fn into_tokens(self) -> Vec<ethabi::Token> {
        self.0
    }
}

impl Detokenize for RawTokens {
    fn from_tokens(tokens: Vec<ethabi::Token>) -> Result<Self, ContractError> {
        Ok(Self(tokens))
    }
}

/// Arguments for calling a function in an unspecified Ethereum smart contract.
#[derive(Debug)]
pub struct CallFunctionArgs {
    pub(crate) name: String,
    pub(crate) from: Option<Address>,
    pub(crate) options: Options,
    pub(crate) block: Option<BlockId>,
    pub(crate) params: RawTokens,
}

impl CallFunctionArgs {
    pub fn new(name: &str, params: impl Tokenize) -> Self {
        Self {
            name: name.to_owned(),
            from: None,
            options: Options::default(),
            block: None,
            params: RawTokens(params.into_tokens()),
        }
    }

    pub fn with_sender(mut self, from: Address) -> Self {
        self.from = Some(from);
        self
    }

    pub fn with_block(mut self, block: BlockId) -> Self {
        self.block = Some(block);
        self
    }

    pub fn for_contract(
        self,
        contract_address: Address,
        contract_abi: ethabi::Contract,
    ) -> ContractCall {
        ContractCall {
            contract_address,
            contract_abi,
            inner: self,
        }
    }
}

/// Information sufficient for calling a function in a specific Ethereum smart contract. Instantiated
/// using [`CallFunctionArgs::for_contract()`].
#[derive(Debug)]
pub struct ContractCall {
    pub(crate) contract_address: Address,
    pub(crate) contract_abi: ethabi::Contract,
    pub(crate) inner: CallFunctionArgs,
}

/// Common error type exposed by the crate,
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Problem on the Ethereum client side (e.g. bad RPC call, network issues).
    #[error("Request to ethereum gateway failed: {0}")]
    EthereumGateway(#[from] zksync_types::web3::Error),
    /// Problem with a contract call.
    #[error("Call to contract failed: {0}")]
    Contract(#[from] zksync_types::web3::contract::Error),
    /// Problem with transaction signer.
    #[error("Transaction signing failed: {0}")]
    Signer(#[from] zksync_eth_signer::error::SignerError),
    /// Problem with transaction decoding.
    #[error("Decoding revert reason failed: {0}")]
    Decode(#[from] ethabi::Error),
    /// Incorrect fee provided for a transaction.
    #[error("Max fee {0} less than priority fee {1}")]
    WrongFeeProvided(U256, U256),
}

/// Raw transaction bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct RawTransactionBytes(pub(crate) Vec<u8>);

impl RawTransactionBytes {
    /// Converts raw transaction bytes. It is caller's responsibility to ensure that these bytes
    /// were actually obtained by signing a transaction.
    pub fn new_unchecked(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for RawTransactionBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Representation of a signed transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct SignedCallResult {
    /// Raw transaction bytes.
    raw_tx: RawTransactionBytes,
    /// `max_priority_fee_per_gas` field of transaction (EIP1559).
    pub max_priority_fee_per_gas: U256,
    /// `max_fee_per_gas` field of transaction (EIP1559).
    pub max_fee_per_gas: U256,
    /// `nonce` field of transaction.
    pub nonce: U256,
    /// Transaction hash.
    pub hash: H256,
}

impl SignedCallResult {
    pub fn new(
        raw_tx: RawTransactionBytes,
        max_priority_fee_per_gas: U256,
        max_fee_per_gas: U256,
        nonce: U256,
        hash: H256,
    ) -> Self {
        Self {
            raw_tx,
            max_priority_fee_per_gas,
            max_fee_per_gas,
            nonce,
            hash,
        }
    }

    pub fn raw_tx(&self, blob_tx_sidecar: Option<&EthTxBlobSidecar>) -> RawTransactionBytes {
        match blob_tx_sidecar {
            None => self.raw_tx.clone(),
            Some(sidecar) => encode_blob_tx_with_sidecar(&self.raw_tx, sidecar),
        }
    }
}

fn encode_blob_tx_with_sidecar(
    raw_tx: &RawTransactionBytes,
    sidecar: &EthTxBlobSidecar,
) -> RawTransactionBytes {
    let EthTxBlobSidecar::EthTxBlobSidecarV1(sidecar) = sidecar;
    let blobs_count = sidecar.blobs.len();

    let raw_tx = &raw_tx.0;
    let mut stream_outer = RlpStream::new();
    stream_outer.begin_list(4);
    stream_outer.append_raw(&raw_tx[1..], 1);

    let mut blob_stream = RlpStream::new_list(blobs_count);
    for i in 0..blobs_count {
        blob_stream.append(&sidecar.blobs[i].blob);
    }
    stream_outer.append_raw(&blob_stream.out(), blobs_count);

    let mut commitment_stream = RlpStream::new_list(blobs_count);
    for i in 0..blobs_count {
        commitment_stream.append(&sidecar.blobs[i].commitment);
    }
    stream_outer.append_raw(&commitment_stream.out(), blobs_count);

    let mut proof_stream = RlpStream::new_list(blobs_count);
    for i in 0..blobs_count {
        proof_stream.append(&sidecar.blobs[i].proof);
    }
    stream_outer.append_raw(&proof_stream.out(), blobs_count);

    let tx = [&[0x03], stream_outer.as_raw()].concat();

    let tx = rlp::encode(&tx);

    RawTransactionBytes(tx.to_vec())
}

/// State of the executed Ethereum transaction.
#[derive(Debug, Clone)]
pub struct ExecutedTxStatus {
    /// The hash of the executed L1 transaction.
    pub tx_hash: H256,
    /// Whether transaction was executed successfully or failed.
    pub success: bool,
    /// Receipt for a transaction.
    pub receipt: TransactionReceipt,
}

/// Information about transaction failure.
///
/// Two common reasons for transaction failure are:
/// - Revert
/// - Out of gas.
///
/// This structure tries to provide information about both of them.
#[derive(Debug, Clone)]
pub struct FailureInfo {
    /// RPC error code.
    pub revert_code: i64,
    /// RPC error message (normally, for a reverted transaction it would
    /// include the revert reason).
    pub revert_reason: String,
    /// Amount of gas used by transaction (may be `None` for `eth_call` requests).
    pub gas_used: Option<U256>,
    /// Gas limit of the transaction.
    pub gas_limit: U256,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;
    use zksync_eth_signer::{EthereumSigner, PrivateKeySigner, TransactionParameters};
    use zksync_types::{
        eth_sender::{EthTxBlobSidecarV1, SidecarBlob},
        web3::{self},
        EIP_4844_TX_TYPE, H160, H256, U256, U64,
    };

    use super::*;

    #[test]
    fn raw_tokens_are_compatible_with_actual_call() {
        let vk_contract = zksync_contracts::verifier_contract();
        let args = CallFunctionArgs::new("verificationKeyHash", ());
        let func = vk_contract.function(&args.name).unwrap();
        func.encode_input(&args.params.into_tokens()).unwrap();

        let output_tokens = vec![ethabi::Token::FixedBytes(vec![1; 32])];
        let RawTokens(output_tokens) = RawTokens::from_tokens(output_tokens).unwrap();
        let hash = H256::from_tokens(output_tokens).unwrap();
        assert_eq!(hash, H256::repeat_byte(1));
    }

    #[tokio::test]
    async fn test_generating_signed_raw_transaction_with_4844_sidecar() {
        let private_key =
            H256::from_str("27593fea79697e947890ecbecce7901b0008345e5d7259710d0dd5e500d040be")
                .unwrap();
        let commitment = hex::decode(
"b62c43c192d54a50d79ad47cb10aa757cd73906b329d5869af1bc16ff4db69235d2827b316616e191113b816c16b3e85"
).unwrap();

        let proof =  hex::decode(
"8b81d1d09565b1a5e44eea2c1a00ee8cc1072aea32f7da9ab463e951e1be5c7875f9904be432787a666425c8f0a52178"
        ).unwrap();

        let signer = PrivateKeySigner::new(private_key);

        let mut blob = vec![0u8; 131072];
        for i in 0..12 {
            blob[i] = b'A';
        }
        blob[12] = 0x0a;
        let raw_transaction = TransactionParameters {
            chain_id: 9,
            nonce: 0.into(),
            max_priority_fee_per_gas: U256::from(1),
            gas_price: Some(U256::from(4)),
            to: Some(H160::from_str("0x0000000000000000000000000000000000000001").unwrap()),
            gas: 0x3.into(),
            value: Default::default(),
            data: Default::default(),
            transaction_type: Some(U64::from(EIP_4844_TX_TYPE)),
            access_list: None,
            max_fee_per_gas: U256::from(2),
            max_fee_per_blob_gas: Some(0x4.into()),
            blob_versioned_hashes: Some(vec![H256::from_slice(
                hex::decode("01b68cce5212e20f8e58722e74edba030a2ab712cea72798ab3526a57afc3b6d")
                    .unwrap()
                    .as_ref(),
            )]),
        };
        let raw_tx = signer
            .sign_transaction(raw_transaction.clone())
            .await
            .unwrap();

        let hash = web3::signing::keccak256(&raw_tx).into();
        // Transaction generated with https://github.com/inphi/blob-utils with
        // the above parameters.
        let expected_str = include_str!("testdata/4844_tx_1.txt");
        let expected_str = expected_str.trim();

        let signed_call_result = SignedCallResult::new(
            RawTransactionBytes(raw_tx),
            U256::from(2),
            U256::from(1),
            0.into(),
            hash,
        );

        let raw_tx_str = hex::encode(
            signed_call_result.raw_tx(
                Some(EthTxBlobSidecar::EthTxBlobSidecarV1(EthTxBlobSidecarV1 {
                    blobs: vec![SidecarBlob {
                        blob,
                        commitment,
                        proof,
                    }],
                }))
                .as_ref(),
            ),
        );

        let expected_str = expected_str.to_owned();
        pretty_assertions::assert_eq!(raw_tx_str, expected_str);
    }
}
