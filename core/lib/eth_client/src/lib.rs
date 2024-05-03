use std::fmt;

use async_trait::async_trait;
use zksync_types::{
    eth_sender::EthTxBlobSidecar,
    web3::{
        ethabi,
        types::{
            AccessList, Address, BlockId, BlockNumber, Filter, Log, Transaction,
            TransactionCondition, TransactionReceipt, H160, H256, U256, U64,
        },
    },
    L1ChainId,
};

pub use crate::types::{
    encode_blob_tx_with_sidecar, Block, CallFunctionArgs, ContractCall, Error, ExecutedTxStatus,
    FailureInfo, RawTransactionBytes, SignedCallResult,
};

pub mod clients;
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

/// Common Web3 interface, as seen by the core applications.
/// Encapsulates the raw Web3 interaction, providing a high-level interface.
///
/// ## Trait contents
///
/// This trait contains methods that perform the "abstract" queries to Web3. That is,
/// there are no assumptions about the contract or account that is used to perform the queries.
/// If you want to add a method to this trait, make sure that it doesn't depend on any particular
/// contract or account address. For that, you can use the `BoundEthInterface` trait.
///
/// ## `component` method
///
/// Most of the trait methods support the `component` parameter. This parameter is used to
/// describe the caller of the method. It may be useful to find the component that makes an
/// unnecessary high amount of Web3 calls. Implementations are advice to count invocations
/// per component and expose them to Prometheus.
#[async_trait]
pub trait EthInterface: 'static + Sync + Send + fmt::Debug {
    /// Returns the nonce of the provided account at the specified block.
    async fn nonce_at_for_account(
        &self,
        account: Address,
        block: BlockNumber,
        component: &'static str,
    ) -> Result<U256, Error>;

    /// Collects the base fee history for the specified block range.
    ///
    /// Returns 1 value for each block in range, assuming that these blocks exist.
    /// Will return an error if the `from_block + block_count` is beyond the head block.
    async fn base_fee_history(
        &self,
        from_block: usize,
        block_count: usize,
        component: &'static str,
    ) -> Result<Vec<u64>, Error>;

    /// Returns the `base_fee_per_gas` value for the currently pending L1 block.
    async fn get_pending_block_base_fee_per_gas(
        &self,
        component: &'static str,
    ) -> Result<U256, Error>;

    /// Returns the current gas price.
    async fn get_gas_price(&self, component: &'static str) -> Result<U256, Error>;

    /// Returns the current block number.
    async fn block_number(&self, component: &'static str) -> Result<U64, Error>;

    /// Sends a transaction to the Ethereum network.
    async fn send_raw_tx(&self, tx: RawTransactionBytes) -> Result<H256, Error>;

    /// Fetches the transaction status for a specified transaction hash.
    ///
    /// Returns `Ok(None)` if the transaction is either not found or not executed yet.
    /// Returns `Err` only if the request fails (e.g. due to network issues).
    async fn get_tx_status(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<ExecutedTxStatus>, Error>;

    /// For a reverted transaction, attempts to recover information on the revert reason.
    ///
    /// Returns `Ok(Some)` if the transaction is reverted.
    /// Returns `Ok(None)` if the transaction isn't found, wasn't executed yet, or if it was
    /// executed successfully.
    /// Returns `Err` only if the request fails (e.g. due to network issues).
    async fn failure_reason(&self, tx_hash: H256) -> Result<Option<FailureInfo>, Error>;

    /// Returns the transaction for the specified hash.
    async fn get_tx(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<Transaction>, Error>;

    /// Returns the receipt for the specified transaction hash.
    async fn tx_receipt(
        &self,
        tx_hash: H256,
        component: &'static str,
    ) -> Result<Option<TransactionReceipt>, Error>;

    /// Returns the ETH balance of the specified token for the specified address.
    async fn eth_balance(&self, address: Address, component: &'static str) -> Result<U256, Error>;

    /// Invokes a function on a contract specified by `contract_address` / `contract_abi` using `eth_call`.
    async fn call_contract_function(&self, call: ContractCall)
        -> Result<Vec<ethabi::Token>, Error>;

    /// Returns the logs for the specified filter.
    async fn logs(&self, filter: Filter, component: &'static str) -> Result<Vec<Log>, Error>;

    /// Returns the block header for the specified block number or hash.
    async fn block(
        &self,
        block_id: BlockId,
        component: &'static str,
    ) -> Result<Option<Block<H256>>, Error>;
}

#[cfg(test)]
static_assertions::assert_obj_safe!(EthInterface);

/// An extension of `EthInterface` trait, which is used to perform queries that are bound to
/// a certain contract and account.
///
/// The example use cases for this trait would be:
/// - An operator that sends transactions and interacts with zkSync contract.
/// - A wallet implementation in the SDK that is tied to a user's account.
///
/// When adding a method to this trait,
/// 1. Make sure that it's indeed "bound". If not, add it to the `EthInterface` trait instead.
/// 2. Consider adding the "unbound" version to the `EthInterface` trait and create a default method
/// implementation that invokes `contract` / `contract_addr` / `sender_account` methods.
#[async_trait]
pub trait BoundEthInterface: EthInterface {
    /// ABI of the contract that is used by the implementer.
    fn contract(&self) -> &ethabi::Contract;

    /// Address of the contract that is used by the implementer.
    fn contract_addr(&self) -> H160;

    /// Chain ID of the L1 network the client is *configured* to connected to.
    ///
    /// This value should be externally provided by the user rather than requested from the network
    /// to avoid accidental network mismatch.
    fn chain_id(&self) -> L1ChainId;

    /// Address of the account associated with the object implementing the trait.
    fn sender_account(&self) -> Address;

    /// Returns the certain ERC20 token allowance for the pair (`Self::sender_account()`, `address`).
    async fn allowance_on_account(
        &self,
        token_address: Address,
        address: Address,
        erc20_abi: ethabi::Contract,
    ) -> Result<U256, Error>;

    /// Signs the transaction and sends it to the Ethereum network.
    /// Expected to use credentials associated with `Self::sender_account()`.
    async fn sign_prepared_tx_for_addr(
        &self,
        data: Vec<u8>,
        contract_addr: H160,
        options: Options,
        component: &'static str,
    ) -> Result<SignedCallResult, Error>;

    /// Returns the nonce of the `Self::sender_account()` at the specified block.
    async fn nonce_at(&self, block: BlockNumber, component: &'static str) -> Result<U256, Error> {
        self.nonce_at_for_account(self.sender_account(), block, component)
            .await
    }

    /// Returns the current nonce of the `Self::sender_account()`.
    async fn current_nonce(&self, component: &'static str) -> Result<U256, Error> {
        self.nonce_at(BlockNumber::Latest, component).await
    }

    /// Returns the pending nonce of the `Self::sender_account()`.
    async fn pending_nonce(&self, component: &'static str) -> Result<U256, Error> {
        self.nonce_at(BlockNumber::Pending, component).await
    }

    /// Similar to [`EthInterface::sign_prepared_tx_for_addr`], but is fixed over `Self::contract_addr()`.
    async fn sign_prepared_tx(
        &self,
        data: Vec<u8>,
        options: Options,
        component: &'static str,
    ) -> Result<SignedCallResult, Error> {
        self.sign_prepared_tx_for_addr(data, self.contract_addr(), options, component)
            .await
    }

    /// Returns the ETH balance of `Self::sender_account()`.
    async fn sender_eth_balance(&self, component: &'static str) -> Result<U256, Error> {
        self.eth_balance(self.sender_account(), component).await
    }

    /// Returns the certain ERC20 token allowance for the `Self::sender_account()`.
    async fn allowance(
        &self,
        token_address: Address,
        erc20_abi: ethabi::Contract,
    ) -> Result<U256, Error> {
        self.allowance_on_account(token_address, self.contract_addr(), erc20_abi)
            .await
    }

    /// Invokes a function on a contract specified by `Self::contract()` / `Self::contract_addr()`.
    async fn call_main_contract_function(
        &self,
        args: CallFunctionArgs,
    ) -> Result<Vec<ethabi::Token>, Error> {
        let args = args.for_contract(self.contract_addr(), self.contract().clone());
        self.call_contract_function(args).await
    }

    /// Encodes a function using the `Self::contract()` ABI.
    ///
    /// `params` are tokenized parameters of the function. Most of the time, you can use
    /// [`Tokenize`][tokenize] trait to convert the parameters into tokens.
    ///
    /// [tokenize]: https://docs.rs/web3/latest/web3/contract/tokens/trait.Tokenize.html
    fn encode_tx_data(&self, func: &str, params: Vec<ethabi::Token>) -> Vec<u8> {
        let f = self
            .contract()
            .function(func)
            .expect("failed to get function parameters");

        f.encode_input(&params)
            .expect("failed to encode parameters")
    }
}
