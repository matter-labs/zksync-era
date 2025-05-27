use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    marker::PhantomData,
    sync::{Arc, RwLock, RwLockWriteGuard},
};

use jsonrpsee::{core::ClientError, types::ErrorObject};
use zksync_types::{
    api::FeeHistory,
    eth_sender::EthTxFinalityStatus,
    ethabi,
    web3::{self, contract::Tokenize, BlockId, BlockNumber},
    Address, L2ChainId, SLChainId, EIP_4844_TX_TYPE, H160, H256, U256, U64,
};
use zksync_web3_decl::client::{MockClient, MockClientBuilder, Network, L1, L2};

use crate::{
    types::{ContractCallError, SignedCallResult, SigningError},
    BaseFees, BoundEthInterface, EthInterface, Options, RawTransactionBytes,
};

#[derive(Debug, Clone)]
struct MockTx {
    recipient: Address,
    input: Vec<u8>,
    hash: H256,
    nonce: u64,
    max_fee_per_gas: U256,
    max_priority_fee_per_gas: U256,
}

impl From<Vec<u8>> for MockTx {
    fn from(raw_tx: Vec<u8>) -> Self {
        let is_eip4844 = raw_tx[0] == EIP_4844_TX_TYPE;
        let tx: Vec<u8> = if is_eip4844 {
            // decoding rlp-encoded length
            let len = raw_tx[2] as usize * 256 + raw_tx[3] as usize - 2;
            raw_tx[3..3 + len].to_vec()
        } else {
            raw_tx
        };
        let len = tx.len();
        let recipient = Address::from_slice(&tx[len - 116..len - 96]);
        let max_fee_per_gas = U256::from(&tx[len - 96..len - 64]);
        let max_priority_fee_per_gas = U256::from(&tx[len - 64..len - 32]);
        let nonce = U256::from(&tx[len - 32..]).as_u64();
        let hash = {
            let mut buffer = [0_u8; 32];
            buffer.copy_from_slice(&tx[..32]);
            if is_eip4844 {
                buffer[0] = 0x00;
            }
            buffer.into()
        };

        Self {
            recipient,
            input: tx[32..len - 116].to_vec(),
            nonce,
            hash,
            max_fee_per_gas,
            max_priority_fee_per_gas,
        }
    }
}

impl From<MockTx> for web3::Transaction {
    fn from(tx: MockTx) -> Self {
        Self {
            to: Some(tx.recipient),
            input: tx.input.into(),
            hash: tx.hash,
            nonce: tx.nonce.into(),
            max_fee_per_gas: Some(tx.max_fee_per_gas),
            max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas),
            ..Self::default()
        }
    }
}

#[derive(Debug)]
struct MockExecutedTx {
    receipt: web3::TransactionReceipt,
    success: bool,
}

/// Mutable part of [`MockSettlementLayer`] that needs to be synchronized via an `RwLock`.
#[derive(Debug)]
struct MockSettlementLayerInner {
    pending_block_number: u64,
    final_block_number: u64,
    safe_block_number: u64,
    executed_txs: HashMap<H256, MockExecutedTx>,
    // TxHash to (Tx, block_number)
    sent_txs: HashMap<H256, (MockTx, u64)>,
    current_nonce: u64,
    pending_nonce: u64,
    nonces: BTreeMap<u64, u64>,
    pub sender: Address,
    pub return_error_on_tx_request: bool,
}

impl Default for MockSettlementLayerInner {
    fn default() -> Self {
        Self {
            pending_block_number: 0,
            safe_block_number: 0,
            final_block_number: 0,
            executed_txs: Default::default(),
            sent_txs: Default::default(),
            current_nonce: 0,
            pending_nonce: 0,
            nonces: Default::default(),
            sender: MOCK_SENDER_ACCOUNT,
            return_error_on_tx_request: false,
        }
    }
}

impl MockSettlementLayerInner {
    fn execute_tx(
        &mut self,
        tx_hash: H256,
        success: bool,
        non_ordering_confirmations: bool,
        finality_status: EthTxFinalityStatus,
    ) {
        let (tx, block_number) = &self.sent_txs[&tx_hash];
        let block_number = { *block_number };
        let nonce = self.current_nonce;
        if self.current_nonce <= tx.nonce {
            self.current_nonce = tx.nonce + 1;
        }
        if self.pending_block_number <= block_number {
            self.pending_block_number = block_number + 1;
        }
        match finality_status {
            EthTxFinalityStatus::Pending => {}
            EthTxFinalityStatus::FastFinalized => {
                if self.safe_block_number <= block_number {
                    self.safe_block_number = block_number + 1;
                }
                if self.pending_block_number <= self.safe_block_number {
                    self.pending_block_number = self.safe_block_number;
                }
            }
            EthTxFinalityStatus::Finalized => {
                if self.final_block_number <= block_number {
                    self.final_block_number = block_number + 1;
                }
                if self.safe_block_number <= self.final_block_number {
                    self.safe_block_number = self.final_block_number;
                }
                if self.pending_block_number <= self.safe_block_number {
                    self.pending_block_number = self.safe_block_number;
                }
            }
        };

        tracing::info!("Executing tx with hash {tx_hash:?} at block {}, with finality: {} success: {success}, current nonce: {}",block_number,  finality_status, nonce);

        let tx_nonce = tx.nonce;

        if non_ordering_confirmations {
            if tx_nonce >= nonce {
                self.current_nonce = tx_nonce + 1;
            }
        } else {
            assert_eq!(tx_nonce, nonce, "nonce mismatch");
        }
        self.nonces.insert(self.pending_block_number, nonce + 1);

        let status = MockExecutedTx {
            success,
            receipt: web3::TransactionReceipt {
                gas_used: Some(21000u32.into()),
                block_number: Some(block_number.into()),
                transaction_hash: tx_hash,
                status: Some(U64::from(if success { 1 } else { 0 })),
                ..web3::TransactionReceipt::default()
            },
        };
        self.executed_txs.insert(tx_hash, status);
    }

    fn get_transaction_count(&self, address: Address, block: web3::BlockNumber) -> U256 {
        if address != self.sender {
            unimplemented!("Getting nonce for custom account is not supported");
        }

        match block {
            web3::BlockNumber::Number(block_number) => {
                let mut nonce_range = self.nonces.range(..=block_number.as_u64());
                let (_, &nonce) = nonce_range.next_back().unwrap_or((&0, &0));
                nonce.into()
            }
            web3::BlockNumber::Pending => self.pending_nonce.into(),
            web3::BlockNumber::Latest => self.current_nonce.into(),
            _ => unimplemented!(
                "`nonce_at_for_account()` called with unsupported block number: {block:?}"
            ),
        }
    }

    fn send_raw_transaction(&mut self, tx: web3::Bytes) -> Result<H256, ClientError> {
        let mock_tx = MockTx::from(tx.0);
        let mock_tx_hash = mock_tx.hash;
        tracing::info!("Sending tx with hash {mock_tx_hash:?}");

        if mock_tx.nonce < self.current_nonce {
            let err = ErrorObject::owned(
                101,
                "transaction with the same nonce already processed",
                None::<()>,
            );
            return Err(ClientError::Call(err));
        }

        if mock_tx.nonce == self.pending_nonce {
            self.pending_nonce += 1;
        }
        self.sent_txs
            .insert(mock_tx_hash, (mock_tx, self.pending_block_number));

        if self.return_error_on_tx_request {
            Err(ClientError::Custom("transport error".to_owned()))
        } else {
            Ok(mock_tx_hash)
        }
    }

    /// Processes a transaction-like `eth_call` which is used in `EthInterface::failure_reason()`.
    fn transaction_call(
        &self,
        request: &web3::CallRequest,
        block_id: BlockId,
    ) -> Option<Result<web3::Bytes, ClientError>> {
        if request.gas.is_none() || request.value.is_none() {
            return None;
        }
        let data = request.data.as_ref()?;

        // Check if any of sent transactions match the request parameters
        let executed_tx = self.sent_txs.iter().find_map(|(hash, (tx, _))| {
            if request.to != Some(tx.recipient) || data.0 != tx.input {
                return None;
            }
            let executed_tx = self.executed_txs.get(hash)?;
            let expected_block_number = executed_tx.receipt.block_number.unwrap();
            (block_id == BlockId::Number(expected_block_number.into())).then_some(executed_tx)
        })?;

        Some(if executed_tx.success {
            Ok(web3::Bytes(vec![1]))
        } else {
            // The error code is arbitrary
            Err(ClientError::Call(ErrorObject::owned(
                3,
                "execution reverted: oops",
                None::<()>,
            )))
        })
    }
}

#[derive(Debug)]
pub struct MockExecutedTxHandle<'a> {
    inner: RwLockWriteGuard<'a, MockSettlementLayerInner>,
    tx_hash: H256,
}

impl MockExecutedTxHandle<'_> {
    pub fn with_logs(&mut self, logs: Vec<web3::Log>) -> &mut Self {
        let status = self.inner.executed_txs.get_mut(&self.tx_hash).unwrap();
        status.receipt.logs = logs;
        self
    }
}

type CallHandler =
    dyn Fn(&web3::CallRequest, BlockId) -> Result<ethabi::Token, ClientError> + Send + Sync;

pub trait SupportedMockSLNetwork: Network {
    fn build_client(builder: MockSettlementLayerBuilder<Self>) -> MockClient<Self>;
}

/// Builder for [`MockSettlementLayer`] client.
pub struct MockSettlementLayerBuilder<Net: SupportedMockSLNetwork = L1> {
    max_fee_per_gas: U256,
    max_priority_fee_per_gas: U256,
    base_fee_history: Vec<BaseFees>,
    /// If true, the mock will not check the ordering nonces of the transactions.
    /// This is useful for testing the cases when the transactions are executed out of order.
    non_ordering_confirmations: bool,
    inner: Arc<RwLock<MockSettlementLayerInner>>,
    call_handler: Box<CallHandler>,
    chain_id: u64,
    _network: PhantomData<Net>,
}

impl<Net: SupportedMockSLNetwork> fmt::Debug for MockSettlementLayerBuilder<Net> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MockSettlementLayerBuilder")
            .field("max_fee_per_gas", &self.max_fee_per_gas)
            .field("max_priority_fee_per_gas", &self.max_priority_fee_per_gas)
            .field("base_fee_history", &self.base_fee_history)
            .field(
                "non_ordering_confirmations",
                &self.non_ordering_confirmations,
            )
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<Net: SupportedMockSLNetwork> Default for MockSettlementLayerBuilder<Net> {
    fn default() -> Self {
        Self {
            max_fee_per_gas: 100.into(),
            max_priority_fee_per_gas: 10.into(),
            base_fee_history: vec![],
            non_ordering_confirmations: false,
            inner: Arc::default(),
            call_handler: Box::new(|call, block_id| {
                panic!("Unexpected eth_call: {call:?}, {block_id:?}");
            }),
            chain_id: 9,
            _network: PhantomData,
        }
    }
}

impl<Net: SupportedMockSLNetwork> MockSettlementLayerBuilder<Net> {
    /// Sets fee history for each block in the mocked Ethereum network, starting from the 0th block.
    pub fn with_fee_history(self, history: Vec<BaseFees>) -> Self {
        Self {
            base_fee_history: history,
            ..self
        }
    }

    pub fn with_non_ordering_confirmation(self, non_ordering_confirmations: bool) -> Self {
        Self {
            non_ordering_confirmations,
            ..self
        }
    }

    pub fn with_sender(self, sender: Address) -> Self {
        self.inner.write().unwrap().sender = sender;
        self
    }

    /// Sets the `eth_call` handler. There are "standard" calls that will not be routed to the handler
    /// (e.g., calls to determine transaction failure reason).
    pub fn with_call_handler<F>(self, call_handler: F) -> Self
    where
        F: 'static + Send + Sync + Fn(&web3::CallRequest, BlockId) -> ethabi::Token,
    {
        Self {
            call_handler: Box::new(move |call, block_id| Ok(call_handler(call, block_id))),
            ..self
        }
    }

    /// Same as [`Self::with_call_handler()`], with a difference that the provided closure should return a `Result`.
    /// Thus, it can emulate network errors, reversions etc.
    pub fn with_fallible_call_handler<F>(self, call_handler: F) -> Self
    where
        F: 'static
            + Send
            + Sync
            + Fn(&web3::CallRequest, BlockId) -> Result<ethabi::Token, ClientError>,
    {
        Self {
            call_handler: Box::new(call_handler),
            ..self
        }
    }

    pub fn with_chain_id(self, chain_id: u64) -> Self {
        Self { chain_id, ..self }
    }

    fn get_block_by_number(fee_history: &[BaseFees], number: U64) -> Option<web3::Block<H256>> {
        let excess_blob_gas = Some(0.into()); // Not relevant for tests.
        let base_fee_per_gas = fee_history
            .get(number.as_usize())
            .map(|fees| fees.base_fee_per_gas.into());

        Some(web3::Block {
            number: Some(number),
            excess_blob_gas,
            base_fee_per_gas,
            ..web3::Block::default()
        })
    }

    fn build_client_inner(self, chaind_id: u64, network: Net) -> MockClientBuilder<Net> {
        let call_handler = self.call_handler;

        MockClient::builder(network)
            .method("eth_chainId", move || Ok(U64::from(chaind_id)))
            .method("eth_blockNumber", {
                let inner = self.inner.clone();
                move || Ok(U64::from(inner.read().unwrap().pending_block_number))
            })
            .method("eth_getBlockByNumber", {
                let inner = self.inner.clone();
                move |block_number, full_transactions: bool| {
                    assert!(
                        !full_transactions,
                        "getting blocks with transactions is not mocked"
                    );
                    let number = match block_number {
                        BlockNumber::Pending => {
                            panic!("`eth_getBlockByNumber` called with `pending` block number")
                        }
                        BlockNumber::Latest => {
                            panic!("`eth_getBlockByNumber` called with `latest` block number")
                        }
                        BlockNumber::Earliest => {
                            panic!("`eth_getBlockByNumber` called with `earliest` block number")
                        }
                        BlockNumber::Number(number) => number,
                        BlockNumber::Finalized => inner.read().unwrap().final_block_number.into(),
                        BlockNumber::Safe => inner.read().unwrap().safe_block_number.into(),
                    };
                    Ok(Self::get_block_by_number(&self.base_fee_history, number))
                }
            })
            .method("eth_getTransactionCount", {
                let inner = self.inner.clone();
                move |address, block| {
                    Ok(inner.read().unwrap().get_transaction_count(address, block))
                }
            })
            .method("eth_gasPrice", move || Ok(self.max_fee_per_gas))
            .method("eth_call", {
                let inner = self.inner.clone();
                move |req, block| {
                    if let Some(res) = inner.read().unwrap().transaction_call(&req, block) {
                        return res;
                    }
                    call_handler(&req, block).map(|token| web3::Bytes(ethabi::encode(&[token])))
                }
            })
            .method("eth_sendRawTransaction", {
                let inner = self.inner.clone();
                move |tx_bytes| inner.write().unwrap().send_raw_transaction(tx_bytes)
            })
            .method("eth_getTransactionByHash", {
                let inner = self.inner.clone();
                move |hash: H256| {
                    let txs = &inner.read().unwrap().sent_txs;
                    let Some(tx) = txs.get(&hash) else {
                        return Ok(None);
                    };
                    Ok(Some(web3::Transaction::from(tx.0.clone())))
                }
            })
            .method("eth_getTransactionReceipt", {
                let inner = self.inner.clone();
                move |hash: H256| {
                    let inner = inner.read().unwrap();
                    let status = inner.executed_txs.get(&hash);
                    Ok(status.map(|status| status.receipt.clone()))
                }
            })
    }

    pub fn build(self) -> MockSettlementLayer<Net> {
        MockSettlementLayer {
            max_fee_per_gas: self.max_fee_per_gas,
            max_priority_fee_per_gas: self.max_priority_fee_per_gas,
            non_ordering_confirmations: self.non_ordering_confirmations,
            inner: self.inner.clone(),
            client: Net::build_client(self),
        }
    }
}

fn l2_eth_fee_history(
    base_fee_history: &[BaseFees],
    block_count: U64,
    newest_block: web3::BlockNumber,
) -> FeeHistory {
    let web3::BlockNumber::Number(from_block) = newest_block else {
        panic!("Non-numeric newest block in `eth_feeHistory`");
    };
    let from_block = from_block.as_usize();
    let start_block = from_block.saturating_sub(block_count.as_usize() - 1);

    // duplicates last value to follow `feeHistory` response format, it should return `block_count + 1` values
    let base_fee_per_gas = base_fee_history[start_block..=from_block]
        .iter()
        .chain([&base_fee_history[from_block]])
        .map(|fee| U256::from(fee.base_fee_per_gas))
        .collect();

    // duplicates last value to follow `feeHistory` response format, it should return `block_count + 1` values
    let base_fee_per_blob_gas = base_fee_history[start_block..=from_block]
        .iter()
        .chain([&base_fee_history[from_block]]) // duplicate last value
        .map(|fee| fee.base_fee_per_blob_gas)
        .collect();

    let l2_pubdata_price = base_fee_history[start_block..=from_block]
        .iter()
        .map(|fee| fee.l2_pubdata_price)
        .collect();

    FeeHistory {
        inner: web3::FeeHistory {
            oldest_block: start_block.into(),
            base_fee_per_gas,
            base_fee_per_blob_gas,
            gas_used_ratio: vec![],      // not used
            blob_gas_used_ratio: vec![], // not used
            reward: None,
        },
        l2_pubdata_price,
    }
}

impl SupportedMockSLNetwork for L1 {
    fn build_client(builder: MockSettlementLayerBuilder<Self>) -> MockClient<Self> {
        let base_fee_history = builder.base_fee_history.clone();
        let chain_id = builder.chain_id;
        let net = SLChainId(builder.chain_id).into();

        builder
            .build_client_inner(chain_id, net)
            .method(
                "eth_feeHistory",
                move |block_count: U64, newest_block: web3::BlockNumber, _: Option<Vec<f32>>| {
                    Ok(l2_eth_fee_history(&base_fee_history, block_count, newest_block).inner)
                },
            )
            .build()
    }
}

impl SupportedMockSLNetwork for L2 {
    fn build_client(builder: MockSettlementLayerBuilder<Self>) -> MockClient<Self> {
        let base_fee_history = builder.base_fee_history.clone();
        let chain_id = builder.chain_id;
        let net = L2ChainId::new(builder.chain_id).unwrap().into();

        builder
            .build_client_inner(chain_id, net)
            .method(
                "eth_feeHistory",
                move |block_count: U64, newest_block: web3::BlockNumber, _: Option<Vec<f32>>| {
                    Ok(l2_eth_fee_history(
                        &base_fee_history,
                        block_count,
                        newest_block,
                    ))
                },
            )
            .build()
    }
}

/// Mock settlement layer client.
#[derive(Debug, Clone)]
pub struct MockSettlementLayer<Net: Network = L1> {
    max_fee_per_gas: U256,
    max_priority_fee_per_gas: U256,
    non_ordering_confirmations: bool,
    inner: Arc<RwLock<MockSettlementLayerInner>>,
    client: MockClient<Net>,
}

impl Default for MockSettlementLayer<L1> {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl Default for MockSettlementLayer<L2> {
    fn default() -> Self {
        Self::builder().build()
    }
}

const MOCK_SENDER_ACCOUNT: Address = Address::repeat_byte(0x11);

impl<Net: SupportedMockSLNetwork> MockSettlementLayer<Net> {
    /// Initializes a builder for a [`MockSettlementLayer`] instance.
    pub fn builder() -> MockSettlementLayerBuilder<Net> {
        MockSettlementLayerBuilder::default()
    }

    /// A fake `sha256` hasher, which calculates an `std::hash` instead.
    /// This is done for simplicity, and it's also much faster.
    fn fake_sha256(data: &[u8]) -> H256 {
        use std::{collections::hash_map::DefaultHasher, hash::Hasher};

        let mut hasher = DefaultHasher::new();
        hasher.write(data);
        let result = hasher.finish();
        H256::from_low_u64_ne(result)
    }

    /// Returns the number of transactions sent via this client.
    pub fn sent_tx_count(&self) -> usize {
        self.inner.read().unwrap().sent_txs.len()
    }

    /// Signs a prepared transaction.
    pub fn sign_prepared_tx(
        &self,
        mut raw_tx: Vec<u8>,
        contract_addr: Address,
        options: Options,
    ) -> Result<SignedCallResult, SigningError> {
        let max_fee_per_gas = options.max_fee_per_gas.unwrap_or(self.max_fee_per_gas);
        let max_priority_fee_per_gas = options
            .max_priority_fee_per_gas
            .unwrap_or(self.max_priority_fee_per_gas);
        let nonce = options.nonce.expect("Nonce must be set for every tx");

        // Nonce and `gas_price` are appended to distinguish the same transactions
        // with different gas by their hash in tests.
        raw_tx.extend_from_slice(contract_addr.as_bytes());
        raw_tx.extend_from_slice(&ethabi::encode(&max_fee_per_gas.into_tokens()));
        raw_tx.extend_from_slice(&ethabi::encode(&max_priority_fee_per_gas.into_tokens()));
        raw_tx.extend_from_slice(&ethabi::encode(&nonce.into_tokens()));
        let hash = Self::fake_sha256(&raw_tx); // Okay for test purposes.

        // Concatenate `raw_tx` plus hash for test purposes
        let mut new_raw_tx = hash.as_bytes().to_vec();
        new_raw_tx.extend(raw_tx);
        Ok(SignedCallResult::new(
            RawTransactionBytes(new_raw_tx),
            max_priority_fee_per_gas,
            max_fee_per_gas,
            nonce,
            hash,
        ))
    }

    /// Increments the blocks by a provided `confirmations` and marks the sent transaction
    /// as a success.
    pub fn execute_tx(
        &self,
        tx_hash: H256,
        success: bool,
        finality_status: EthTxFinalityStatus,
    ) -> MockExecutedTxHandle<'_> {
        let mut inner = self.inner.write().unwrap();
        inner.execute_tx(
            tx_hash,
            success,
            self.non_ordering_confirmations,
            finality_status,
        );
        MockExecutedTxHandle { inner, tx_hash }
    }

    /// Revert the block. It will set proper block numbers and
    /// remove all executed transactions in reverted blocks.
    pub fn revert_block_by_number(&self, val: u64) -> u64 {
        let mut inner = self.inner.write().unwrap();
        let pending_block_number = inner.pending_block_number;
        let final_block_number = inner.final_block_number;
        let allowed_blocks_to_revert = pending_block_number - final_block_number;
        if allowed_blocks_to_revert < val {
            panic!(
                "Cannot revert block number by {val}, only {allowed_blocks_to_revert} blocks left"
            );
        };

        let desired_pending_block_number = pending_block_number - val;
        inner.pending_block_number = desired_pending_block_number;
        inner.safe_block_number =
            std::cmp::min(inner.safe_block_number, desired_pending_block_number);
        inner
            .nonces
            .retain(|&block_number, _| block_number <= desired_pending_block_number);

        let current_nonce = inner.nonces.values().max().cloned().unwrap_or_default();

        inner.current_nonce = current_nonce;

        let txs: Vec<_> = inner
            .sent_txs
            .iter()
            .filter_map(|(hash, (_, block_number))| {
                if *block_number >= desired_pending_block_number {
                    Some(*hash)
                } else {
                    None
                }
            })
            .collect();

        for tx_hash in txs.iter() {
            inner.executed_txs.remove(tx_hash);
            inner.sent_txs.remove(tx_hash);
        }

        desired_pending_block_number
    }

    /// Increases the block number in the network by the specified value.
    pub fn advance_block_number(&self, val: u64, finality_status: EthTxFinalityStatus) -> u64 {
        let mut inner = self.inner.write().unwrap();

        match finality_status {
            EthTxFinalityStatus::Pending => {
                inner.pending_block_number += val;
                inner.pending_block_number
            }
            EthTxFinalityStatus::FastFinalized => {
                inner.safe_block_number += val;
                inner.safe_block_number
            }
            EthTxFinalityStatus::Finalized => {
                inner.pending_block_number += val;
                inner.safe_block_number = inner.pending_block_number;
                inner.final_block_number = inner.safe_block_number;
                inner.final_block_number
            }
        }
    }

    /// Converts this client into an immutable / contract-agnostic client.
    pub fn into_client(self) -> MockClient<Net> {
        self.client
    }

    pub fn set_return_error_on_tx_request(&self, value: bool) {
        self.inner.write().unwrap().return_error_on_tx_request = value;
    }
}

impl<T: SupportedMockSLNetwork> AsRef<dyn EthInterface> for MockSettlementLayer<T> {
    fn as_ref(&self) -> &(dyn EthInterface + 'static) {
        &self.client
    }
}

#[async_trait::async_trait]
impl<Net: SupportedMockSLNetwork + SupportedMockSLNetwork> BoundEthInterface
    for MockSettlementLayer<Net>
{
    fn clone_boxed(&self) -> Box<dyn BoundEthInterface> {
        Box::new(self.clone())
    }

    fn for_component(self: Box<Self>, _component_name: &'static str) -> Box<dyn BoundEthInterface> {
        self
    }

    fn contract(&self) -> &ethabi::Contract {
        unimplemented!("Not needed right now")
    }

    fn contract_addr(&self) -> H160 {
        H160::repeat_byte(0x22)
    }

    fn chain_id(&self) -> SLChainId {
        SLChainId(505)
    }

    fn sender_account(&self) -> Address {
        self.inner.read().unwrap().sender
    }

    async fn sign_prepared_tx_for_addr(
        &self,
        data: Vec<u8>,
        contract_addr: H160,
        options: Options,
    ) -> Result<SignedCallResult, SigningError> {
        self.sign_prepared_tx(data, contract_addr, options)
    }

    async fn allowance_on_account(
        &self,
        _token_address: Address,
        _contract_address: Address,
        _erc20_abi: &ethabi::Contract,
    ) -> Result<U256, ContractCallError> {
        unimplemented!("Not needed right now")
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use zksync_types::{commitment::L1BatchCommitmentMode, ProtocolVersionId};

    use super::*;
    use crate::{CallFunctionArgs, EthFeeInterface, EthInterface};

    fn base_fees(block: u64, blob: u64, pubdata_price: u64) -> BaseFees {
        BaseFees {
            base_fee_per_gas: block,
            base_fee_per_blob_gas: U256::from(blob),
            l2_pubdata_price: U256::from(pubdata_price),
        }
    }

    #[tokio::test]
    async fn managing_block_number() {
        let mock = MockSettlementLayer::<L1>::builder()
            .with_fee_history(vec![
                base_fees(0, 4, 0),
                base_fees(1, 3, 0),
                base_fees(2, 2, 0),
                base_fees(3, 1, 0),
                base_fees(4, 0, 0),
            ])
            .build();
        let block_number = mock.client.block_number().await.unwrap();
        assert_eq!(block_number, 0.into());

        mock.advance_block_number(5, EthTxFinalityStatus::Finalized);
        let block_number = mock.client.block_number().await.unwrap();
        assert_eq!(block_number, 5.into());

        for number in 0..=4 {
            let block_number = web3::BlockNumber::Number(number.into()).into();
            let block = mock
                .client
                .block(block_number)
                .await
                .unwrap()
                .expect("no block");
            assert_eq!(block.number, Some(number.into()));
            assert_eq!(block.base_fee_per_gas.unwrap(), U256::from(number));
        }
    }

    #[tokio::test]
    async fn getting_chain_id() {
        let mock = MockSettlementLayer::<L1>::builder().build();
        let chain_id = mock.client.fetch_chain_id().await.unwrap();
        assert_eq!(chain_id, SLChainId(9));
    }

    #[tokio::test]
    async fn managing_fee_history() {
        let initial_fee_history = vec![
            base_fees(1, 4, 0),
            base_fees(2, 3, 0),
            base_fees(3, 2, 0),
            base_fees(4, 1, 0),
            base_fees(5, 0, 0),
        ];
        let client = MockSettlementLayer::<L1>::builder()
            .with_fee_history(initial_fee_history.clone())
            .build();
        client.advance_block_number(4, EthTxFinalityStatus::Finalized);

        let fee_history = client.client.base_fee_history(4, 4).await.unwrap();
        assert_eq!(fee_history, initial_fee_history[1..=4]);
        let fee_history = client.client.base_fee_history(2, 2).await.unwrap();
        assert_eq!(fee_history, initial_fee_history[1..=2]);
        let fee_history = client.client.base_fee_history(3, 2).await.unwrap();
        assert_eq!(fee_history, initial_fee_history[2..=3]);
    }

    #[tokio::test]
    async fn managing_fee_history_l2() {
        let initial_fee_history = vec![
            base_fees(1, 0, 11),
            base_fees(2, 0, 12),
            base_fees(3, 0, 13),
            base_fees(4, 0, 14),
            base_fees(5, 0, 15),
        ];
        let client = MockSettlementLayer::<L2>::builder()
            .with_fee_history(initial_fee_history.clone())
            .build();
        client.advance_block_number(4, EthTxFinalityStatus::Finalized);

        let fee_history = client.client.base_fee_history(4, 4).await.unwrap();
        assert_eq!(fee_history, initial_fee_history[1..=4]);
        let fee_history = client.client.base_fee_history(2, 2).await.unwrap();
        assert_eq!(fee_history, initial_fee_history[1..=2]);
        let fee_history = client.client.base_fee_history(3, 2).await.unwrap();
        assert_eq!(fee_history, initial_fee_history[2..=3]);
    }

    #[tokio::test]
    async fn managing_transactions() {
        let client = MockSettlementLayer::<L1>::builder()
            .with_non_ordering_confirmation(true)
            .build();
        client.advance_block_number(2, EthTxFinalityStatus::Finalized);

        let signed_tx = client
            .sign_prepared_tx(
                b"test".to_vec(),
                Address::repeat_byte(1),
                Options {
                    nonce: Some(1.into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(signed_tx.nonce, 1.into());
        assert!(signed_tx.max_priority_fee_per_gas > 0.into());
        assert!(signed_tx.max_fee_per_gas > 0.into());

        let tx_hash = client
            .as_ref()
            .send_raw_tx(signed_tx.raw_tx.clone())
            .await
            .unwrap();
        assert_eq!(tx_hash, signed_tx.hash);

        client.execute_tx(tx_hash, true, EthTxFinalityStatus::Finalized);
        let returned_tx = client
            .as_ref()
            .get_tx(tx_hash)
            .await
            .unwrap()
            .expect("no transaction");
        assert_eq!(returned_tx.hash, tx_hash);
        assert_eq!(returned_tx.to, Some(Address::repeat_byte(1)));
        assert_eq!(returned_tx.input.0, b"test");
        assert_eq!(returned_tx.nonce, 1.into());
        assert!(returned_tx.max_priority_fee_per_gas.is_some());
        assert!(returned_tx.max_fee_per_gas.is_some());

        let tx_status = client
            .as_ref()
            .get_tx_status(tx_hash)
            .await
            .unwrap()
            .expect("no transaction status");
        assert!(tx_status.success);
        assert_eq!(tx_status.tx_hash, tx_hash);
        assert_eq!(tx_status.receipt.block_number, Some(2.into()));
    }

    #[tokio::test]
    async fn calling_contracts() {
        let client = MockSettlementLayer::<L1>::builder()
            .with_call_handler(|req, _block_id| {
                let packed_semver = ProtocolVersionId::latest().into_packed_semver_with_patch(0);
                let call_signature = &req.data.as_ref().unwrap().0[..4];
                let contract = zksync_contracts::hyperchain_contract();
                let pricing_mode_sig = contract
                    .function("getPubdataPricingMode")
                    .unwrap()
                    .short_signature();
                let protocol_version_sig = contract
                    .function("getProtocolVersion")
                    .unwrap()
                    .short_signature();
                match call_signature {
                    sig if sig == pricing_mode_sig => {
                        ethabi::Token::Uint(0.into()) // "rollup" mode encoding
                    }
                    sig if sig == protocol_version_sig => ethabi::Token::Uint(packed_semver),
                    _ => panic!("unexpected call"),
                }
            })
            .build();

        let l1_packed_protocol_version: U256 = CallFunctionArgs::new("getProtocolVersion", ())
            .for_contract(
                client.contract_addr(),
                &zksync_contracts::hyperchain_contract(),
            )
            .call(client.as_ref())
            .await
            .unwrap();
        let expected_packed_protocol_version =
            ProtocolVersionId::latest().into_packed_semver_with_patch(0);
        assert_eq!(l1_packed_protocol_version, expected_packed_protocol_version);

        let commitment_mode: L1BatchCommitmentMode =
            CallFunctionArgs::new("getPubdataPricingMode", ())
                .for_contract(
                    client.contract_addr(),
                    &zksync_contracts::hyperchain_contract(),
                )
                .call(client.as_ref())
                .await
                .unwrap();
        assert_matches!(commitment_mode, L1BatchCommitmentMode::Rollup);
    }

    #[tokio::test]
    async fn getting_transaction_failure_reason() {
        let client = MockSettlementLayer::<L1>::default();
        let signed_tx = client
            .sign_prepared_tx(
                vec![1, 2, 3],
                Address::repeat_byte(1),
                Options {
                    nonce: Some(0.into()),
                    ..Options::default()
                },
            )
            .unwrap();
        let tx_hash = client.as_ref().send_raw_tx(signed_tx.raw_tx).await.unwrap();
        assert_eq!(tx_hash, signed_tx.hash);

        client.execute_tx(tx_hash, true, EthTxFinalityStatus::Finalized);
        let failure = client.as_ref().failure_reason(tx_hash).await.unwrap();
        assert!(failure.is_none(), "{failure:?}");

        let signed_tx = client
            .sign_prepared_tx(
                vec![4, 5, 6],
                Address::repeat_byte(0xff),
                Options {
                    nonce: Some(1.into()),
                    ..Options::default()
                },
            )
            .unwrap();
        let failed_tx_hash = client.as_ref().send_raw_tx(signed_tx.raw_tx).await.unwrap();
        assert_ne!(failed_tx_hash, tx_hash);

        client.execute_tx(failed_tx_hash, false, EthTxFinalityStatus::Finalized);
        let failure = client
            .as_ref()
            .failure_reason(failed_tx_hash)
            .await
            .unwrap()
            .expect("no failure");
        assert_eq!(failure.revert_reason, "oops");
        assert_eq!(failure.revert_code, 3);
    }
}
