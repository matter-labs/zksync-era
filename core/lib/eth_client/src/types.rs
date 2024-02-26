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
    EIP_4844_TX_TYPE,
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
    /// EIP4844 transaction lacks `max_fee_per_blob_gas` field
    #[error("EIP4844 transaction lacks max_fee_per_blob_gas field")]
    Eip4844MissingMaxFeePerBlobGas,
    /// EIP4844 transaction lacks `blob_versioned_hashes` field
    #[error("EIP4844 transaction lacks blob_versioned_hashes field")]
    Eip4844MissingBlobVersionedHashes,
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
    pub raw_tx: RawTransactionBytes,
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
}

// Encodes the blob transaction and the blob sidecar into the networking
// format as defined in <https://eips.ethereum.org/EIPS/eip-4844#networking>
pub(crate) fn encode_blob_tx_with_sidecar(raw_tx: &[u8], sidecar: &EthTxBlobSidecar) -> Vec<u8> {
    let EthTxBlobSidecar::EthTxBlobSidecarV1(sidecar) = sidecar;
    let blobs_count = sidecar.blobs.len();

    let mut stream_outer = RlpStream::new();

    // top-level RLP encoded struct is defined as
    //
    // ```
    // rlp([tx_payload_body, blobs, commitments, proofs])
    // ```
    // and is a list of 4 elements.
    stream_outer.begin_list(4);
    // The EIP doc states the following:
    //
    // ```
    // the EIP-2718 TransactionPayload of the blob transaction
    // is wrapped to become:
    //
    // rlp([tx_payload_body, blobs, commitments, proofs])
    // ```
    //
    // If you look into the specs what this means for us here is the following:
    //
    // 1. The `0x03` byte signaling the type of the blob transaction has
    //    to be removed from the head of received payload body
    // 2. The above four-element RLP list has to be constructed.
    // 3. The `0x03` byte has to be concatenated with the RLP-encoded list from
    //    the previous step
    // 4. The result of this concatenation has to be again RLP-encoded into
    //    what constitutes the final form of a blob transaction with the sidecar
    //    as it is sent to the network.
    stream_outer.append_raw(&raw_tx[1..], 1);

    let mut blob_stream = RlpStream::new_list(blobs_count);
    let mut commitment_stream = RlpStream::new_list(blobs_count);
    let mut proof_stream = RlpStream::new_list(blobs_count);

    for i in 0..blobs_count {
        blob_stream.append(&sidecar.blobs[i].blob);
        commitment_stream.append(&sidecar.blobs[i].commitment);
        proof_stream.append(&sidecar.blobs[i].proof);
    }

    stream_outer.append_raw(&blob_stream.out(), 1);
    stream_outer.append_raw(&commitment_stream.out(), 1);
    stream_outer.append_raw(&proof_stream.out(), 1);

    let tx = [&[EIP_4844_TX_TYPE], stream_outer.as_raw()].concat();

    tx
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
        eth_sender::{EthTxBlobSidecarV1, SidecarBlobV1},
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
    // Tests the encoding of the `EIP_4844_TX_TYPE` transaction to
    // network format defined by the `EIP`. That is, a signed transaction
    // itself and the sidecar containing the blobs.
    async fn test_generating_signed_raw_transaction_with_4844_sidecar() {
        let private_key =
            H256::from_str("27593fea79697e947890ecbecce7901b0008345e5d7259710d0dd5e500d040be")
                .unwrap();
        let commitment = hex::decode(
            "b5022d2a994ebd05f42c2f8e9b227185bf5963fcd1d412e17e97a026698d9670c0139872a400740d25835b5eaade22ad"
        ).unwrap();

        let versioned_hash =
            hex::decode("01a034bbe3f441bdce53ea4bcf4aa3c7561bc03d4559fdb88e1d33a868e5c56e")
                .unwrap();

        let proof =  hex::decode(
"9996e250c444400385d1ec7ef99a365d18eeed5d2d6eff969c0b7dcdea7829a309fe93e6678160599a5832f64619cbca"
        ).unwrap();

        let signer = PrivateKeySigner::new(private_key);

        // A blob we want to test is the 12 bytes `'A'`. It gets
        // expanded to the constant size of the blob since blobs in
        // EIP4844 transactions are of constant size.
        let mut blob = vec![0u8; 131072];

        // set the blob first 12 bytes to `'A'`.
        for item in blob.iter_mut().take(12) {
            *item = b'A';
        }

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
                hex::decode("01a034bbe3f441bdce53ea4bcf4aa3c7561bc03d4559fdb88e1d33a868e5c56e")
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
        let raw_tx = encode_blob_tx_with_sidecar(
            &raw_tx,
            &EthTxBlobSidecar::EthTxBlobSidecarV1(EthTxBlobSidecarV1 {
                blobs: vec![SidecarBlobV1 {
                    blob,
                    commitment,
                    proof,
                    versioned_hash,
                }],
            }),
        );

        let signed_call_result = SignedCallResult::new(
            RawTransactionBytes(raw_tx),
            U256::from(2),
            U256::from(1),
            0.into(),
            hash,
        );

        let raw_tx_str = hex::encode(signed_call_result.raw_tx);

        let expected_str = expected_str.to_owned();
        pretty_assertions::assert_eq!(raw_tx_str, expected_str);
    }

    #[tokio::test]
    // Tests the encoding of the `EIP_4844_TX_TYPE` transaction to
    // network format defined by the `EIP`. That is, a signed transaction
    // itself and the sidecar containing the blobs.
    async fn test_generating_signed_raw_transaction_with_4844_sidecar_two_blobs() {
        let private_key =
            H256::from_str("27593fea79697e947890ecbecce7901b0008345e5d7259710d0dd5e500d040be")
                .unwrap();

        let commitment_1 = hex::decode(
"b5022d2a994ebd05f42c2f8e9b227185bf5963fcd1d412e17e97a026698d9670c0139872a400740d25835b5eaade22ad"
        ).unwrap();
        let commitment_2 = hex::decode(
"b23cc16159b670c04d1407f2117e026890e69516ca275c26831f33da88b5a0f717a2357b89b091d5cba4b4d6396079c4"
        ).unwrap();

        let proof_1 = hex::decode(
"9996e250c444400385d1ec7ef99a365d18eeed5d2d6eff969c0b7dcdea7829a309fe93e6678160599a5832f64619cbca" 
            ).unwrap();
        let proof_2 = hex::decode(
"a5f5961ea0128c49513f5713d40abd88c41d1fbd73b88bc41a7936bef276051b92856891fd4f24561b0197e9e4bd6816"
            ).unwrap();

        let versioned_hash_1 = H256::from_slice(
            hex::decode("01a034bbe3f441bdce53ea4bcf4aa3c7561bc03d4559fdb88e1d33a868e5c56e")
                .unwrap()
                .as_ref(),
        );
        let versioned_hash_2 = H256::from_slice(
            hex::decode("01f968e77bb7aef1cacba266547362eb7465f2f7a071b407af99704ea33b667d")
                .unwrap()
                .as_ref(),
        );

        let signer = PrivateKeySigner::new(private_key);

        // Two blobs we want to test are:
        //  1. the 12 bytes are set to `'A'`.
        //  2. the 12 bytes are set to `'B'`.
        //
        //
        // They get expanded to the constant size of the blob since blobs in
        // EIP4844 transactions are of constant size.
        let mut blob_1 = vec![0u8; 131072];
        let mut blob_2 = vec![0u8; 131072];

        // set the blobs first 12 bytes to `'A'` and `'B'` respectively.
        for i in 0..12 {
            blob_1[i] = b'A';
            blob_2[i] = b'B';
        }

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
            blob_versioned_hashes: Some(vec![versioned_hash_1, versioned_hash_2]),
        };

        let raw_tx = signer
            .sign_transaction(raw_transaction.clone())
            .await
            .unwrap();

        let hash = web3::signing::keccak256(&raw_tx).into();
        // Transaction generated with https://github.com/inphi/blob-utils with
        // the above parameters.
        let expected_str = include_str!("testdata/4844_tx_2.txt");
        let expected_str = expected_str.trim();
        let raw_tx = encode_blob_tx_with_sidecar(
            &raw_tx,
            &EthTxBlobSidecar::EthTxBlobSidecarV1(EthTxBlobSidecarV1 {
                blobs: vec![
                    SidecarBlobV1 {
                        blob: blob_1,
                        commitment: commitment_1,
                        proof: proof_1,
                        versioned_hash: versioned_hash_1.to_fixed_bytes().to_vec(),
                    },
                    SidecarBlobV1 {
                        blob: blob_2,
                        commitment: commitment_2,
                        proof: proof_2,
                        versioned_hash: versioned_hash_2.to_fixed_bytes().to_vec(),
                    },
                ],
            }),
        );

        let signed_call_result = SignedCallResult::new(
            RawTransactionBytes(raw_tx),
            U256::from(2),
            U256::from(1),
            0.into(),
            hash,
        );

        let raw_tx_str = hex::encode(signed_call_result.raw_tx);

        let expected_str = expected_str.to_owned();
        pretty_assertions::assert_eq!(raw_tx_str, expected_str);
    }
}
