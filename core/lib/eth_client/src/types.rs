use zksync_types::web3::{
    contract::{
        tokens::{Detokenize, Tokenize},
        Error as ContractError, Options,
    },
    ethabi,
    types::{Address, BlockId, TransactionReceipt, H256, U256},
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
}
