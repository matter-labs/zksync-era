use std::{cmp::max, fmt};

use async_trait::async_trait;
use zksync_types::{
    eth_sender::EthTxBlobSidecar,
    ethabi,
    transaction_request::PaymasterParams,
    web3,
    web3::{
        AccessList, Block, BlockId, BlockNumber, Filter, Log, Transaction, TransactionCondition,
        TransactionReceipt,
    },
    Address, SLChainId, H160, H256, U256, U64,
};
#[cfg(feature = "node_framework")]
use zksync_web3_decl::node::SettlementLayerClient;
pub use zksync_web3_decl::{
    self as web3_decl,
    error::{EnrichedClientError, EnrichedClientResult},
    jsonrpsee::core::ClientError,
};

pub use crate::types::{
    encode_blob_tx_with_sidecar, CallFunctionArgs, ContractCall, ContractCallError,
    ExecutedTxStatus, FailureInfo, RawTransactionBytes, SignedCallResult, SigningError,
};

pub mod clients;
pub mod contracts_loader;
#[cfg(feature = "node_framework")]
pub mod node;
mod types;

/// Contract Call/Query Options
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Options {
    /// Fixed gas limit
    pub gas: Option<U256>,
    /// Fixed gas price
    pub gas_price: Option<U256>,
    /// Value to transfer
    pub value: Option<U256>,
    /// Fixed transaction nonce
    pub nonce: Option<U256>,
    /// A condition to satisfy before including transaction.
    pub condition: Option<TransactionCondition>,
    /// Transaction type, Some(1) for AccessList transaction, None for Legacy
    pub transaction_type: Option<U64>,
    /// Access list
    pub access_list: Option<AccessList>,
    /// Max fee per gas
    pub max_fee_per_gas: Option<U256>,
    /// miner bribe
    pub max_priority_fee_per_gas: Option<U256>,
    /// Max fee per blob gas
    pub max_fee_per_blob_gas: Option<U256>,
    /// Blob versioned hashes
    pub blob_versioned_hashes: Option<Vec<H256>>,
    /// Blob sidecar
    pub blob_tx_sidecar: Option<EthTxBlobSidecar>,
    // EIP 712 params
    // Max Gas per pubdata
    pub max_gas_per_pubdata: Option<U256>,
    // Factory deps
    pub factory_deps: Option<Vec<Vec<u8>>>,
    // Paymaster params
    pub paymaster_params: Option<PaymasterParams>,
}

impl Options {
    /// Create new default `Options` object with some modifications.
    pub fn with<F>(func: F) -> Options
    where
        F: FnOnce(&mut Options),
    {
        let mut options = Options::default();
        func(&mut options);
        options
    }
}

/// Information about the base fees provided by the L1 client.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BaseFees {
    pub base_fee_per_gas: u64,
    // Base fee per blob gas. It is zero on networks that do not support blob transactions (e.g. L2s).
    pub base_fee_per_blob_gas: U256,
    // The price (in wei) for relaying the pubdata to L1. It is non-zero only for L2 settlement layers.
    pub l2_pubdata_price: U256,
}

impl BaseFees {
    pub fn gas_per_pubdata(&self) -> u64 {
        self.l2_pubdata_price
            .as_u64()
            .div_ceil(max(1, self.base_fee_per_gas))
    }
}

/// Common Web3 interface, as seen by the core applications.
///
/// Encapsulates the raw Web3 interaction, providing a high-level interface. Acts as an extension
/// trait implemented for L1 / Ethereum [clients](zksync_web3_decl::client::Client).
///
/// ## Trait contents
///
/// This trait contains methods that perform the "abstract" queries to Web3. That is,
/// there are no assumptions about the contract or account that is used to perform the queries.
/// If you want to add a method to this trait, make sure that it doesn't depend on any particular
/// contract or account address. For that, you can use the `BoundEthInterface` trait.
#[async_trait]
pub trait EthInterface: Sync + Send + fmt::Debug {
    /// Fetches the L1 chain ID (in contrast to [`BoundEthInterface::chain_id()`] which returns
    /// the *expected* L1 chain ID).
    async fn fetch_chain_id(&self) -> EnrichedClientResult<SLChainId>;

    /// Returns the nonce of the provided account at the specified block.
    async fn nonce_at_for_account(
        &self,
        account: Address,
        block: BlockNumber,
    ) -> EnrichedClientResult<U256>;

    /// Returns the `base_fee_per_gas` value for the currently pending L1 block.
    async fn get_pending_block_base_fee_per_gas(&self) -> EnrichedClientResult<U256>;

    /// Returns the current gas price.
    async fn get_gas_price(&self) -> EnrichedClientResult<U256>;

    /// Returns the current block number.
    async fn block_number(&self) -> EnrichedClientResult<U64>;

    /// Sends a transaction to the Ethereum network.
    async fn send_raw_tx(&self, tx: RawTransactionBytes) -> EnrichedClientResult<H256>;

    /// Fetches the transaction status for a specified transaction hash.
    ///
    /// Returns `Ok(None)` if the transaction is either not found or not executed yet.
    /// Returns `Err` only if the request fails (e.g. due to network issues).
    async fn get_tx_status(&self, hash: H256) -> EnrichedClientResult<Option<ExecutedTxStatus>>;

    /// For a reverted transaction, attempts to recover information on the revert reason.
    ///
    /// Returns `Ok(Some)` if the transaction is reverted.
    /// Returns `Ok(None)` if the transaction isn't found, wasn't executed yet, or if it was
    /// executed successfully.
    /// Returns `Err` only if the request fails (e.g. due to network issues).
    async fn failure_reason(&self, tx_hash: H256) -> EnrichedClientResult<Option<FailureInfo>>;

    /// Returns the transaction for the specified hash.
    async fn get_tx(&self, hash: H256) -> EnrichedClientResult<Option<Transaction>>;

    /// Returns the receipt for the specified transaction hash.
    async fn tx_receipt(&self, tx_hash: H256) -> EnrichedClientResult<Option<TransactionReceipt>>;

    /// Returns the ETH balance of the specified token for the specified address.
    async fn eth_balance(&self, address: Address) -> EnrichedClientResult<U256>;

    /// Invokes a function on a contract specified by `contract_address` / `contract_abi` using `eth_call`.
    async fn call_contract_function(
        &self,
        request: web3::CallRequest,
        block: Option<BlockId>,
    ) -> EnrichedClientResult<web3::Bytes>;

    /// Returns the logs for the specified filter.
    async fn logs(&self, filter: &Filter) -> EnrichedClientResult<Vec<Log>>;

    /// Returns the block header for the specified block number or hash.
    async fn block(&self, block_id: BlockId) -> EnrichedClientResult<Option<Block<H256>>>;
}

#[cfg(feature = "node_framework")]
impl From<SettlementLayerClient> for Box<dyn EthInterface> {
    fn from(client: SettlementLayerClient) -> Self {
        match client {
            SettlementLayerClient::L1(client) => Box::new(client),
            SettlementLayerClient::Gateway(client) => Box::new(client),
        }
    }
}

#[async_trait::async_trait]
pub trait EthFeeInterface: EthInterface {
    /// Collects the base fee history for the specified block range.
    ///
    /// Returns 1 value for each block in range, assuming that these blocks exist.
    /// Will return an error if the `from_block + block_count` is beyond the head block.
    async fn base_fee_history(
        &self,
        from_block: usize,
        block_count: usize,
    ) -> EnrichedClientResult<Vec<BaseFees>>;
}

/// An extension of `EthInterface` trait, which is used to perform queries that are bound to
/// a certain contract and account.
///
/// The example use cases for this trait would be:
///
/// - An operator that sends transactions and interacts with ZKsync contract.
/// - A wallet implementation in the SDK that is tied to a user's account.
///
/// When adding a method to this trait:
///
/// 1. Make sure that it's indeed "bound". If not, add it to the `EthInterface` trait instead.
/// 2. Consider adding the "unbound" version to the `EthInterface` trait and create a default method
///   implementation that invokes `contract` / `contract_addr` / `sender_account` methods.
#[async_trait]
pub trait BoundEthInterface: 'static + Sync + Send + fmt::Debug + AsRef<dyn EthInterface> {
    /// Clones this client.
    fn clone_boxed(&self) -> Box<dyn BoundEthInterface>;

    /// Tags this client as working for a specific component. The component name can be used in logging,
    /// metrics etc. The component name should be copied to the clones of this client, but should not be passed upstream.
    fn for_component(self: Box<Self>, component_name: &'static str) -> Box<dyn BoundEthInterface>;

    /// ABI of the contract that is used by the implementer.
    fn contract(&self) -> &ethabi::Contract;

    /// Address of the contract that is used by the implementer.
    fn contract_addr(&self) -> H160;

    /// Chain ID of the L1 network the client is *configured* to connected to.
    ///
    /// This value should be externally provided by the user rather than requested from the network
    /// to avoid accidental network mismatch.
    fn chain_id(&self) -> SLChainId;

    /// Address of the account associated with the object implementing the trait.
    fn sender_account(&self) -> Address;

    /// Returns the certain ERC20 token allowance for the pair (`Self::sender_account()`, `address`).
    async fn allowance_on_account(
        &self,
        token_address: Address,
        address: Address,
        erc20_abi: &ethabi::Contract,
    ) -> Result<U256, ContractCallError>;

    /// Signs the transaction and sends it to the Ethereum network.
    /// Expected to use credentials associated with `Self::sender_account()`.
    async fn sign_prepared_tx_for_addr(
        &self,
        data: Vec<u8>,
        contract_addr: H160,
        options: Options,
    ) -> Result<SignedCallResult, SigningError>;
}

impl Clone for Box<dyn BoundEthInterface> {
    fn clone(&self) -> Self {
        self.clone_boxed()
    }
}

impl dyn BoundEthInterface {
    /// Returns the nonce of the `Self::sender_account()` at the specified block.
    pub async fn nonce_at(&self, block: BlockNumber) -> EnrichedClientResult<U256> {
        self.as_ref()
            .nonce_at_for_account(self.sender_account(), block)
            .await
    }

    /// Returns the current nonce of the `Self::sender_account()`.
    pub async fn current_nonce(&self) -> EnrichedClientResult<U256> {
        self.nonce_at(BlockNumber::Latest).await
    }

    /// Returns the pending nonce of the `Self::sender_account()`.
    pub async fn pending_nonce(&self) -> EnrichedClientResult<U256> {
        self.nonce_at(BlockNumber::Pending).await
    }

    /// Similar to [`EthInterface::sign_prepared_tx_for_addr`], but is fixed over `Self::contract_addr()`.
    pub async fn sign_prepared_tx(
        &self,
        data: Vec<u8>,
        options: Options,
    ) -> Result<SignedCallResult, SigningError> {
        self.sign_prepared_tx_for_addr(data, self.contract_addr(), options)
            .await
    }

    /// Returns the ETH balance of `Self::sender_account()`.
    pub async fn sender_eth_balance(&self) -> EnrichedClientResult<U256> {
        self.as_ref().eth_balance(self.sender_account()).await
    }

    /// Encodes a function using the `Self::contract()` ABI.
    ///
    /// `params` are tokenized parameters of the function. Most of the time, you can use
    /// [`Tokenize`][tokenize] trait to convert the parameters into tokens.
    ///
    /// [tokenize]: https://docs.rs/web3/latest/web3/contract/tokens/trait.Tokenize.html
    pub fn encode_tx_data(&self, func: &str, params: Vec<ethabi::Token>) -> Vec<u8> {
        let f = self
            .contract()
            .function(func)
            .expect("failed to get function parameters");

        f.encode_input(&params)
            .expect("failed to encode parameters")
    }
}
