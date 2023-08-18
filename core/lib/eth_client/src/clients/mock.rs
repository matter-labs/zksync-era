use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use jsonrpc_core::types::error::Error as RpcError;
use std::collections::{BTreeMap, HashMap};
use std::sync::RwLock;
use zksync_types::{
    web3::{
        contract::{
            tokens::{Detokenize, Tokenize},
            Options,
        },
        ethabi::{self, Token},
        types::{Block, BlockId, BlockNumber, Filter, Log, Transaction, TransactionReceipt, U64},
        Error as Web3Error,
    },
    Address, L1ChainId, ProtocolVersionId, H160, H256, U256,
};

use crate::{
    types::{Error, ExecutedTxStatus, FailureInfo, SignedCallResult},
    BoundEthInterface, EthInterface,
};

#[derive(Debug, Clone, Default, Copy)]
pub struct MockTx {
    pub hash: H256,
    pub nonce: u64,
    pub base_fee: U256,
}

impl From<Vec<u8>> for MockTx {
    fn from(tx: Vec<u8>) -> Self {
        use std::convert::TryFrom;

        let len = tx.len();
        let total_gas_price = U256::try_from(&tx[len - 96..len - 64]).unwrap();
        let priority_fee = U256::try_from(&tx[len - 64..len - 32]).unwrap();
        let base_fee = total_gas_price - priority_fee;
        let nonce = U256::try_from(&tx[len - 32..]).unwrap().as_u64();
        let hash = {
            let mut buffer: [u8; 32] = Default::default();
            buffer.copy_from_slice(&tx[..32]);
            buffer.into()
        };

        Self {
            nonce,
            hash,
            base_fee,
        }
    }
}

/// Mock Ethereum client is capable of recording all the incoming requests for the further analysis.
#[derive(Debug)]
pub struct MockEthereum {
    pub block_number: AtomicU64,
    pub max_fee_per_gas: U256,
    pub base_fee_history: RwLock<Vec<u64>>,
    pub max_priority_fee_per_gas: U256,
    pub tx_statuses: RwLock<HashMap<H256, ExecutedTxStatus>>,
    pub sent_txs: RwLock<HashMap<H256, MockTx>>,
    pub current_nonce: AtomicU64,
    pub pending_nonce: AtomicU64,
    pub nonces: RwLock<BTreeMap<u64, u64>>,
    /// If true, the mock will not check the ordering nonces of the transactions.
    /// This is useful for testing the cases when the transactions are executed out of order.
    pub non_ordering_confirmations: bool,
    pub multicall_address: Address,
}

impl Default for MockEthereum {
    fn default() -> Self {
        Self {
            max_fee_per_gas: 100.into(),
            max_priority_fee_per_gas: 10.into(),
            block_number: Default::default(),
            base_fee_history: Default::default(),
            tx_statuses: Default::default(),
            sent_txs: Default::default(),
            current_nonce: Default::default(),
            pending_nonce: Default::default(),
            nonces: RwLock::new([(0, 0)].into()),
            non_ordering_confirmations: false,
            multicall_address: Address::default(),
        }
    }
}

impl MockEthereum {
    /// A fake `sha256` hasher, which calculates an `std::hash` instead.
    /// This is done for simplicity and it's also much faster.
    pub fn fake_sha256(data: &[u8]) -> H256 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;

        let mut hasher = DefaultHasher::new();
        hasher.write(data);

        let result = hasher.finish();

        H256::from_low_u64_ne(result)
    }

    /// Increments the blocks by a provided `confirmations` and marks the sent transaction
    /// as a success.
    pub fn execute_tx(
        &self,
        tx_hash: H256,
        success: bool,
        confirmations: u64,
    ) -> anyhow::Result<()> {
        let block_number = self.block_number.fetch_add(confirmations, Ordering::SeqCst);
        let nonce = self.current_nonce.fetch_add(1, Ordering::SeqCst);
        let tx_nonce = self.sent_txs.read().unwrap()[&tx_hash].nonce;

        if self.non_ordering_confirmations {
            if tx_nonce >= nonce {
                self.current_nonce.store(tx_nonce, Ordering::SeqCst);
            }
        } else {
            anyhow::ensure!(tx_nonce == nonce, "nonce mismatch");
        }

        self.nonces.write().unwrap().insert(block_number, nonce + 1);

        let status = ExecutedTxStatus {
            tx_hash,
            success,
            receipt: TransactionReceipt {
                gas_used: Some(21000u32.into()),
                block_number: Some(block_number.into()),
                transaction_hash: tx_hash,
                ..Default::default()
            },
        };

        self.tx_statuses.write().unwrap().insert(tx_hash, status);

        Ok(())
    }

    pub fn sign_prepared_tx(
        &self,
        mut raw_tx: Vec<u8>,
        options: Options,
    ) -> Result<SignedCallResult, Error> {
        let max_fee_per_gas = options.max_fee_per_gas.unwrap_or(self.max_fee_per_gas);
        let max_priority_fee_per_gas = options
            .max_priority_fee_per_gas
            .unwrap_or(self.max_priority_fee_per_gas);
        let nonce = options.nonce.expect("Nonce must be set for every tx");

        // Nonce and gas_price are appended to distinguish the same transactions
        // with different gas by their hash in tests.
        raw_tx.append(&mut ethabi::encode(&max_fee_per_gas.into_tokens()));
        raw_tx.append(&mut ethabi::encode(&max_priority_fee_per_gas.into_tokens()));
        raw_tx.append(&mut ethabi::encode(&nonce.into_tokens()));
        let hash = Self::fake_sha256(&raw_tx); // Okay for test purposes.

        // Concatenate raw_tx plus hash for test purposes
        let mut new_raw_tx = hash.as_bytes().to_vec();
        new_raw_tx.extend(raw_tx);
        Ok(SignedCallResult {
            raw_tx: new_raw_tx,
            max_priority_fee_per_gas,
            max_fee_per_gas,
            nonce,
            hash,
        })
    }

    pub fn advance_block_number(&self, val: u64) -> u64 {
        self.block_number.fetch_add(val, Ordering::SeqCst) + val
    }

    pub fn with_fee_history(self, history: Vec<u64>) -> Self {
        Self {
            base_fee_history: RwLock::new(history),
            ..self
        }
    }

    pub fn with_non_ordering_confirmation(self, non_ordering_confirmations: bool) -> Self {
        Self {
            non_ordering_confirmations,
            ..self
        }
    }

    pub fn with_multicall_address(self, address: Address) -> Self {
        Self {
            multicall_address: address,
            ..self
        }
    }
}

#[async_trait]
impl EthInterface for MockEthereum {
    async fn get_tx_status(
        &self,
        hash: H256,
        _: &'static str,
    ) -> Result<Option<ExecutedTxStatus>, Error> {
        Ok(self.tx_statuses.read().unwrap().get(&hash).cloned())
    }

    async fn block_number(&self, _: &'static str) -> Result<U64, Error> {
        Ok(self.block_number.load(Ordering::SeqCst).into())
    }

    async fn send_raw_tx(&self, tx: Vec<u8>) -> Result<H256, Error> {
        let mock_tx = MockTx::from(tx);

        if mock_tx.nonce < self.current_nonce.load(Ordering::SeqCst) {
            return Err(Error::EthereumGateway(Web3Error::Rpc(RpcError {
                message: "transaction with the same nonce already processed".to_string(),
                code: 101.into(),
                data: None,
            })));
        }

        if mock_tx.nonce == self.pending_nonce.load(Ordering::SeqCst) {
            self.pending_nonce.fetch_add(1, Ordering::SeqCst);
        }

        self.sent_txs.write().unwrap().insert(mock_tx.hash, mock_tx);

        Ok(mock_tx.hash)
    }

    async fn nonce_at_for_account(
        &self,
        _account: Address,
        _block: BlockNumber,
        _: &'static str,
    ) -> Result<U256, Error> {
        unimplemented!("Getting nonce for custom account is not supported")
    }

    async fn get_gas_price(&self, _: &'static str) -> Result<U256, Error> {
        Ok(self.max_fee_per_gas)
    }

    async fn base_fee_history(
        &self,
        from_block: usize,
        block_count: usize,
        _component: &'static str,
    ) -> Result<Vec<u64>, Error> {
        Ok(self.base_fee_history.read().unwrap()
            [from_block.saturating_sub(block_count - 1)..=from_block]
            .to_vec())
    }

    async fn get_pending_block_base_fee_per_gas(
        &self,
        _component: &'static str,
    ) -> Result<U256, Error> {
        Ok(U256::from(
            *self.base_fee_history.read().unwrap().last().unwrap(),
        ))
    }

    async fn failure_reason(&self, tx_hash: H256) -> Result<Option<FailureInfo>, Error> {
        let tx_status = self.get_tx_status(tx_hash, "failure_reason").await.unwrap();

        Ok(tx_status.map(|status| FailureInfo {
            revert_code: status.success as i64,
            revert_reason: "Unknown".into(),
            gas_used: status.receipt.gas_used,
            gas_limit: U256::zero(),
        }))
    }

    #[allow(clippy::too_many_arguments)]
    async fn call_contract_function<R, A, B, P>(
        &self,
        _func: &str,
        _params: P,
        _from: A,
        _options: Options,
        _block: B,
        contract_address: Address,
        _contract_abi: ethabi::Contract,
    ) -> Result<R, Error>
    where
        R: Detokenize + Unpin,
        A: Into<Option<Address>> + Send,
        B: Into<Option<BlockId>> + Send,
        P: Tokenize + Send,
    {
        if contract_address == self.multicall_address {
            let token = Token::Array(vec![
                Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![1u8; 32])]),
                Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![2u8; 32])]),
                Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![3u8; 96])]),
                Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![4u8; 32])]),
                Token::Tuple(vec![
                    Token::Bool(true),
                    Token::Bytes(
                        H256::from_low_u64_be(ProtocolVersionId::default() as u64)
                            .0
                            .to_vec(),
                    ),
                ]),
            ]);
            return Ok(R::from_tokens(vec![token]).unwrap());
        }
        Ok(R::from_tokens(vec![]).unwrap())
    }

    async fn get_tx(
        &self,
        _hash: H256,
        _component: &'static str,
    ) -> Result<Option<Transaction>, Error> {
        unimplemented!("Not needed right now")
    }

    async fn tx_receipt(
        &self,
        _tx_hash: H256,
        _component: &'static str,
    ) -> Result<Option<TransactionReceipt>, Error> {
        unimplemented!("Not needed right now")
    }

    async fn eth_balance(
        &self,
        _address: Address,
        _component: &'static str,
    ) -> Result<U256, Error> {
        unimplemented!("Not needed right now")
    }

    async fn logs(&self, _filter: Filter, _component: &'static str) -> Result<Vec<Log>, Error> {
        unimplemented!("Not needed right now")
    }

    async fn block(
        &self,
        _block_id: String,
        _component: &'static str,
    ) -> Result<Option<Block<H256>>, Error> {
        unimplemented!("Not needed right now")
    }
}

#[async_trait::async_trait]
impl BoundEthInterface for MockEthereum {
    fn contract(&self) -> &ethabi::Contract {
        unimplemented!("Not needed right now")
    }

    fn contract_addr(&self) -> H160 {
        H160::repeat_byte(0x22)
    }

    fn chain_id(&self) -> L1ChainId {
        unimplemented!("Not needed right now")
    }

    fn sender_account(&self) -> Address {
        Address::repeat_byte(0x11)
    }

    async fn sign_prepared_tx_for_addr(
        &self,
        data: Vec<u8>,
        _contract_addr: H160,
        options: Options,
        _component: &'static str,
    ) -> Result<SignedCallResult, Error> {
        self.sign_prepared_tx(data, options)
    }

    async fn allowance_on_account(
        &self,
        _token_address: Address,
        _contract_address: Address,
        _erc20_abi: ethabi::Contract,
    ) -> Result<U256, Error> {
        unimplemented!("Not needed right now")
    }

    async fn nonce_at(&self, block: BlockNumber, _component: &'static str) -> Result<U256, Error> {
        if let BlockNumber::Number(block_number) = block {
            Ok((*self
                .nonces
                .read()
                .unwrap()
                .range(..=block_number.as_u64())
                .next_back()
                .unwrap()
                .1)
                .into())
        } else {
            panic!("MockEthereum::nonce_at called with non-number block tag");
        }
    }

    async fn pending_nonce(&self, _: &'static str) -> Result<U256, Error> {
        Ok(self.pending_nonce.load(Ordering::SeqCst).into())
    }

    async fn current_nonce(&self, _: &'static str) -> Result<U256, Error> {
        Ok(self.current_nonce.load(Ordering::SeqCst).into())
    }
}

#[async_trait]
impl<T: AsRef<MockEthereum> + Send + Sync> EthInterface for T {
    async fn nonce_at_for_account(
        &self,
        account: Address,
        block: BlockNumber,
        component: &'static str,
    ) -> Result<U256, Error> {
        self.as_ref()
            .nonce_at_for_account(account, block, component)
            .await
    }

    async fn base_fee_history(
        &self,
        from_block: usize,
        block_count: usize,
        component: &'static str,
    ) -> Result<Vec<u64>, Error> {
        self.as_ref()
            .base_fee_history(from_block, block_count, component)
            .await
    }

    async fn get_pending_block_base_fee_per_gas(
        &self,
        component: &'static str,
    ) -> Result<U256, Error> {
        self.as_ref()
            .get_pending_block_base_fee_per_gas(component)
            .await
    }

    async fn get_gas_price(&self, component: &'static str) -> Result<U256, Error> {
        self.as_ref().get_gas_price(component).await
    }

    async fn block_number(&self, component: &'static str) -> Result<U64, Error> {
        self.as_ref().block_number(component).await
    }

    async fn send_raw_tx(&self, tx: Vec<u8>) -> Result<H256, Error> {
        self.as_ref().send_raw_tx(tx).await
    }

    async fn failure_reason(&self, tx_hash: H256) -> Result<Option<FailureInfo>, Error> {
        self.as_ref().failure_reason(tx_hash).await
    }

    async fn get_tx_status(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<ExecutedTxStatus>, Error> {
        self.as_ref().get_tx_status(hash, component).await
    }

    async fn get_tx(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<Transaction>, Error> {
        self.as_ref().get_tx(hash, component).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn call_contract_function<R, A, B, P>(
        &self,
        func: &str,
        params: P,
        from: A,
        options: Options,
        block: B,
        contract_address: Address,
        contract_abi: ethabi::Contract,
    ) -> Result<R, Error>
    where
        R: Detokenize + Unpin,
        A: Into<Option<Address>> + Send,
        B: Into<Option<BlockId>> + Send,
        P: Tokenize + Send,
    {
        self.as_ref()
            .call_contract_function(
                func,
                params,
                from,
                options,
                block,
                contract_address,
                contract_abi,
            )
            .await
    }

    async fn tx_receipt(
        &self,
        tx_hash: H256,
        component: &'static str,
    ) -> Result<Option<TransactionReceipt>, Error> {
        self.as_ref().tx_receipt(tx_hash, component).await
    }

    async fn eth_balance(&self, address: Address, component: &'static str) -> Result<U256, Error> {
        self.as_ref().eth_balance(address, component).await
    }

    async fn logs(&self, filter: Filter, component: &'static str) -> Result<Vec<Log>, Error> {
        self.as_ref().logs(filter, component).await
    }

    async fn block(
        &self,
        block_id: String,
        component: &'static str,
    ) -> Result<Option<Block<H256>>, Error> {
        self.as_ref().block(block_id, component).await
    }
}

#[async_trait::async_trait]
impl<T: AsRef<MockEthereum> + Send + Sync> BoundEthInterface for T {
    fn contract(&self) -> &ethabi::Contract {
        self.as_ref().contract()
    }

    fn contract_addr(&self) -> H160 {
        self.as_ref().contract_addr()
    }

    fn chain_id(&self) -> L1ChainId {
        self.as_ref().chain_id()
    }

    fn sender_account(&self) -> Address {
        self.as_ref().sender_account()
    }

    async fn sign_prepared_tx_for_addr(
        &self,
        data: Vec<u8>,
        contract_addr: H160,
        options: Options,
        component: &'static str,
    ) -> Result<SignedCallResult, Error> {
        self.as_ref()
            .sign_prepared_tx_for_addr(data, contract_addr, options, component)
            .await
    }

    async fn allowance_on_account(
        &self,
        token_address: Address,
        contract_address: Address,
        erc20_abi: ethabi::Contract,
    ) -> Result<U256, Error> {
        self.as_ref()
            .allowance_on_account(token_address, contract_address, erc20_abi)
            .await
    }

    async fn nonce_at(&self, block: BlockNumber, component: &'static str) -> Result<U256, Error> {
        self.as_ref().nonce_at(block, component).await
    }

    async fn pending_nonce(&self, _: &'static str) -> Result<U256, Error> {
        self.as_ref().pending_nonce("").await
    }

    async fn current_nonce(&self, _: &'static str) -> Result<U256, Error> {
        self.as_ref().current_nonce("").await
    }
}
