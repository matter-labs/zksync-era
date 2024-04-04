use rlp::RlpStream;
use serde::{Deserialize, Deserializer, Serialize};
use zksync_types::{
    eth_sender::EthTxBlobSidecar,
    ethabi::ethereum_types::H64,
    web3::{
        contract::{
            tokens::{Detokenize, Tokenize},
            Error as ContractError, Options,
        },
        ethabi,
        types::{Address, BlockId, BlockNumber, TransactionReceipt, H256, U256},
    },
    Bytes, EIP_4844_TX_TYPE, H160, H2048, U64,
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

impl ContractCall {
    pub fn contract_address(&self) -> Address {
        self.contract_address
    }

    pub fn function_name(&self) -> &str {
        &self.inner.name
    }

    pub fn args(&self) -> &[ethabi::Token] {
        &self.inner.params.0
    }
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
pub fn encode_blob_tx_with_sidecar(raw_tx: &[u8], sidecar: &EthTxBlobSidecar) -> Vec<u8> {
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

/// The fee history type returned from RPC calls.
/// We don't use `FeeHistory` from `web3` crate because it's not compatible
/// with node implementations that skip empty `base_fee_per_gas`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeHistory {
    /// Lowest number block of the returned range.
    pub oldest_block: BlockNumber,
    /// A vector of block base fees per gas. This includes the next block after the newest of the returned range, because this value can be derived from the newest block. Zeroes are returned for pre-EIP-1559 blocks.
    pub base_fee_per_gas: Option<Vec<U256>>,
    /// A vector of block gas used ratios. These are calculated as the ratio of gas used and gas limit.
    pub gas_used_ratio: Option<Vec<f64>>,
    /// A vector of effective priority fee per gas data points from a single block. All zeroes are returned if the block is empty. Returned only if requested.
    pub reward: Option<Vec<Vec<U256>>>,
}

/// The block type returned from RPC calls.
/// This is generic over a `TX` type.
/// We can't use `Block` from `web3` crate because it doesn't support `excessBlobGas`.
#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub struct Block<TX> {
    /// Hash of the block
    pub hash: Option<H256>,
    /// Hash of the parent
    #[serde(rename = "parentHash")]
    pub parent_hash: H256,
    /// Hash of the uncles
    #[serde(rename = "sha3Uncles")]
    #[cfg_attr(feature = "allow-missing-fields", serde(default))]
    pub uncles_hash: H256,
    /// Author's address.
    #[serde(rename = "miner", default, deserialize_with = "null_to_default")]
    pub author: H160,
    /// State root hash
    #[serde(rename = "stateRoot")]
    pub state_root: H256,
    /// Transactions root hash
    #[serde(rename = "transactionsRoot")]
    pub transactions_root: H256,
    /// Transactions receipts root hash
    #[serde(rename = "receiptsRoot")]
    pub receipts_root: H256,
    /// Block number. None if pending.
    pub number: Option<U64>,
    /// Gas Used
    #[serde(rename = "gasUsed")]
    pub gas_used: U256,
    /// Gas Limit
    #[serde(rename = "gasLimit")]
    #[cfg_attr(feature = "allow-missing-fields", serde(default))]
    pub gas_limit: U256,
    /// Base fee per unit of gas (if past London)
    #[serde(rename = "baseFeePerGas", skip_serializing_if = "Option::is_none")]
    pub base_fee_per_gas: Option<U256>,
    /// Extra data
    #[serde(rename = "extraData")]
    pub extra_data: Bytes,
    /// Logs bloom
    #[serde(rename = "logsBloom")]
    pub logs_bloom: Option<H2048>,
    /// Timestamp
    pub timestamp: U256,
    /// Difficulty
    #[cfg_attr(feature = "allow-missing-fields", serde(default))]
    pub difficulty: U256,
    /// Total difficulty
    #[serde(rename = "totalDifficulty")]
    pub total_difficulty: Option<U256>,
    /// Seal fields
    #[serde(default, rename = "sealFields")]
    pub seal_fields: Vec<Bytes>,
    /// Uncles' hashes
    #[cfg_attr(feature = "allow-missing-fields", serde(default))]
    pub uncles: Vec<H256>,
    /// Transactions
    pub transactions: Vec<TX>,
    /// Size in bytes
    pub size: Option<U256>,
    /// Mix Hash
    #[serde(rename = "mixHash")]
    pub mix_hash: Option<H256>,
    /// Nonce
    pub nonce: Option<H64>,
    /// Excess blob gas
    #[serde(rename = "excessBlobGas")]
    pub excess_blob_gas: Option<U64>,
}

fn null_to_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    let option = Option::deserialize(deserializer)?;
    Ok(option.unwrap_or_default())
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

    #[test]
    fn block_can_be_deserialized() {
        let post_dencun = r#"
        {
            "baseFeePerGas": "0x3e344c311",
            "blobGasUsed": "0xc0000",
            "difficulty": "0x0",
            "excessBlobGas": "0x4b40000",
            "extraData": "0xd883010d0d846765746888676f312e32302e34856c696e7578",
            "gasLimit": "0x1c9c380",
            "gasUsed": "0x96070e",
            "hash": "0x2c77691707319e5b8a5afde5b75c77ec0ccb1f556c1ccd9e0e49ef5795bc9015",
            "logsBloom": "0x08a212844c2050e028d95b42b718a550c7215100101028408e90520820c81c1171400212a0200483f89c89010a0604934650804fc0a283a06d011041a22c000296e0880394850081700ac41a156812a59ea5de0518a43bcb0000a865c242c42348f570a9a3704350c484424d0400ab04000805e0840094680028a218c0099dc2ca42b01c63224045510375050860308894480901e8a6d0e818035158805218400e493001c9a8060a205c1112611442040804041fc00088cca49e262b068000349cf611ebc61c1b00220282150c16096c1370921412420815008398200041721d12d2988ea090d0429208010564809845080808707a1d3052f343020e88091221",
            "miner": "0x0000000000000000000000000000000000000000",
            "mixHash": "0xcae6acf2f1e34499c234db81aff35299979046ab0e544276005899ce5f320858",
            "nonce": "0x0000000000000000",
            "number": "0x51fca7",
            "parentBeaconBlockRoot": "0x08750d6ce3bf47639efc14c43f44fc1c018d593f1b48aacd222668422ed289ce",
            "parentHash": "0x2f3978cb0dec7ecbb01fff2be87b101cf61e9bfb31d64c15a19aca7c4f74c87e",
            "receiptsRoot": "0xdad010222b2ce0862e928402b2d10d9651d78d9276c8ad30bc9c0793aba9dd18",
            "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
            "size": "0xa862",
            "stateRoot": "0xf77a819b26fef9fe5c7cd9511b0fff950725e878cb311914de162be068fb8c70",
            "timestamp": "0x65ddadfc",
            "totalDifficulty": "0x3c656d23029ab0",
            "transactions": ["0x8660a77cc4f13681e039b8f980579c53f522ba13aad64481a5bdfe5f62b54bbb", "0xab5820f72a6374db701a5e27668f14dac867ade71806bb754b561a63a9441df4"],
            "transactionsRoot": "0xd9f4b814ee427db492c4fcc249df665b616db61bd423413ad7ffd9b464aced1e",
            "uncles": [],
            "withdrawals": [{
                "address": "0xe276bc378a527a8792b353cdca5b5e53263dfb9e",
                "amount": "0x24e0",
                "index": "0x2457a70",
                "validatorIndex": "0x3df"
            }, {
                "address": "0xe276bc378a527a8792b353cdca5b5e53263dfb9e",
                "amount": "0x24e0",
                "index": "0x2457a71",
                "validatorIndex": "0x3e1"
            }],
            "withdrawalsRoot": "0x576ea3c5cc0bf9d73f64e9786e7b44595a62a76a54989acc8ae84b2015fde929"
        }"#;
        let block: Block<H256> = serde_json::from_str(post_dencun).unwrap();
        assert_eq!(block.excess_blob_gas, Some(U64::from(0x4b40000)));

        let pre_dencun = r#"
        {
            "baseFeePerGas": "0x910fe39fd",
            "difficulty": "0x0",
            "extraData": "0x546974616e2028746974616e6275696c6465722e78797a29",
            "gasLimit": "0x1c9c380",
            "gasUsed": "0x10d5e31",
            "hash": "0x40a92d343f724efbc2c85b710a433391b9102813560704ec7db89572a3b23451",
            "logsBloom": "0x15a15083ea8148119f3845a4a7f01c291a1393153ec36e21ecdc60cffea60c55c16397c190e48eb7efa27f96500959541a91c251a8e0aeca5ed0942a4dec8a118b6a25bdd9248c6b8b53556ed62c0be8daec00f984ec196f5faa4f52cae5ceb76ade34648e73ac6371bed5ecd1491fc7083a2d75ea88e7e0f7448d56969cb8069fc09a768d1a93f1833f81c1050e36e465220cf99f9502bbc4e0b5475d52b8e1ea819ecb7bacb8baeb16d8f59b9904cc171b970d0834c8fecd6291df1514dae1ff474e939c5af95773a013f6926d1dd5915591d28e18201daf707923f417a9ead1796959c3cfe6facf0c85a7a620aac8a44616eb484db2d78039fb606de4549d",
            "miner": "0x4838b106fce9647bdf1e7877bf73ce8b0bad5f97",
            "mixHash": "0x041736203c26f01d68a6a3b5728205cc27cb1674fdad8cc85dd722933483b446",
            "nonce": "0x0000000000000000",
            "number": "0x126c531",
            "parentHash": "0x57535175149b25b4258e6de43e7484f6b6f144dcddbb0add5c2f25e700c57508",
            "receiptsRoot": "0x8cc94702aca57e058aaa5da92e7e776f76ac67191a638c5664844deb59616c8f",
            "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
            "size": "0x3bfca",
            "stateRoot": "0xbeb859bb91bc80792f6640ff469837927dc5a7ad5393484b2e1b536f3815af2f",
            "timestamp": "0x65ddae8b",
            "totalDifficulty": "0xc70d815d562d3cfa955",
            "transactions": ["0x61b183ea82664e6afb05d3fa692f71561381e5c9280e80062770ff88d4c3c8be", "0x3dd322f75e4315dd265c22a8ae3e51bd32253f43664d1dc4d2afb68746da7e70"],
            "transactionsRoot": "0xba2b8ff471608f8877abc6d06f7f599e3c942671b11598b0595fc13185681458",
            "uncles": [],
            "withdrawals": [{
                "address": "0xb9d7934878b5fb9610b3fe8a5e441e8fad7e293f",
                "amount": "0x1168dd0",
                "index": "0x22d6606",
                "validatorIndex": "0x5c797"
            }, {
                "address": "0xb9d7934878b5fb9610b3fe8a5e441e8fad7e293f",
                "amount": "0x117bae7",
                "index": "0x22d6607",
                "validatorIndex": "0x5c798"
            }],
            "withdrawalsRoot": "0xf3386eae1beb91726fe62e2aba4b975ed4f3be2222c739bf2b4d72dd20324a8b"
        }"#;
        let block: Block<H256> = serde_json::from_str(pre_dencun).unwrap();
        assert!(block.excess_blob_gas.is_none());
    }
}
