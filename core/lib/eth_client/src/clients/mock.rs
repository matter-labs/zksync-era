use std::{
    collections::{BTreeMap, HashMap},
    sync::RwLock,
};

use async_trait::async_trait;
use jsonrpc_core::types::error::Error as RpcError;
use zksync_types::{
    web3::{
        contract::tokens::Tokenize,
        ethabi,
        types::{BlockId, BlockNumber, Filter, Log, Transaction, TransactionReceipt, U64},
        Error as Web3Error,
    },
    Address, L1ChainId, ProtocolVersionId, H160, H256, U256,
};

use crate::{
    types::{Error, ExecutedTxStatus, FailureInfo, SignedCallResult},
    Block, BoundEthInterface, ContractCall, EthInterface, Options, RawTransactionBytes,
};

#[derive(Debug, Clone)]
struct MockTx {
    input: Vec<u8>,
    hash: H256,
    nonce: u64,
    max_fee_per_gas: U256,
    max_priority_fee_per_gas: U256,
}

impl From<Vec<u8>> for MockTx {
    fn from(tx: Vec<u8>) -> Self {
        let len = tx.len();
        let max_fee_per_gas = U256::try_from(&tx[len - 96..len - 64]).unwrap();
        let max_priority_fee_per_gas = U256::try_from(&tx[len - 64..len - 32]).unwrap();
        let nonce = U256::try_from(&tx[len - 32..]).unwrap().as_u64();
        let hash = {
            let mut buffer = [0_u8; 32];
            buffer.copy_from_slice(&tx[..32]);
            buffer.into()
        };

        Self {
            input: tx[32..len - 96].to_vec(),
            nonce,
            hash,
            max_fee_per_gas,
            max_priority_fee_per_gas,
        }
    }
}

impl From<MockTx> for Transaction {
    fn from(tx: MockTx) -> Self {
        Self {
            input: tx.input.into(),
            hash: tx.hash,
            nonce: tx.nonce.into(),
            max_fee_per_gas: Some(tx.max_fee_per_gas),
            max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas),
            ..Self::default()
        }
    }
}

/// Mutable part of [`MockEthereum`] that needs to be synchronized via an `RwLock`.
#[derive(Debug, Default)]
struct MockEthereumInner {
    block_number: u64,
    tx_statuses: HashMap<H256, ExecutedTxStatus>,
    sent_txs: HashMap<H256, MockTx>,
    current_nonce: u64,
    pending_nonce: u64,
    nonces: BTreeMap<u64, u64>,
}

impl MockEthereumInner {
    fn execute_tx(
        &mut self,
        tx_hash: H256,
        success: bool,
        confirmations: u64,
        non_ordering_confirmations: bool,
    ) {
        let block_number = self.block_number;
        self.block_number += confirmations;
        let nonce = self.current_nonce;
        self.current_nonce += 1;
        let tx_nonce = self.sent_txs[&tx_hash].nonce;

        if non_ordering_confirmations {
            if tx_nonce >= nonce {
                self.current_nonce = tx_nonce;
            }
        } else {
            assert_eq!(tx_nonce, nonce, "nonce mismatch");
        }
        self.nonces.insert(block_number, nonce + 1);

        let status = ExecutedTxStatus {
            tx_hash,
            success,
            receipt: TransactionReceipt {
                gas_used: Some(21000u32.into()),
                block_number: Some(block_number.into()),
                transaction_hash: tx_hash,
                ..TransactionReceipt::default()
            },
        };
        self.tx_statuses.insert(tx_hash, status);
    }
}

/// Mock Ethereum client is capable of recording all the incoming requests for the further analysis.
#[derive(Debug)]
pub struct MockEthereum {
    max_fee_per_gas: U256,
    max_priority_fee_per_gas: U256,
    base_fee_history: Vec<u64>,
    excess_blob_gas_history: Vec<u64>,
    /// If true, the mock will not check the ordering nonces of the transactions.
    /// This is useful for testing the cases when the transactions are executed out of order.
    non_ordering_confirmations: bool,
    multicall_address: Address,
    inner: RwLock<MockEthereumInner>,
}

impl Default for MockEthereum {
    fn default() -> Self {
        Self {
            max_fee_per_gas: 100.into(),
            max_priority_fee_per_gas: 10.into(),
            base_fee_history: vec![],
            excess_blob_gas_history: vec![],
            non_ordering_confirmations: false,
            multicall_address: Address::default(),
            inner: RwLock::default(),
        }
    }
}

impl MockEthereum {
    /// A fake `sha256` hasher, which calculates an `std::hash` instead.
    /// This is done for simplicity and it's also much faster.
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

    /// Increments the blocks by a provided `confirmations` and marks the sent transaction
    /// as a success.
    pub fn execute_tx(&self, tx_hash: H256, success: bool, confirmations: u64) {
        self.inner.write().unwrap().execute_tx(
            tx_hash,
            success,
            confirmations,
            self.non_ordering_confirmations,
        );
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

        // Nonce and `gas_price` are appended to distinguish the same transactions
        // with different gas by their hash in tests.
        raw_tx.append(&mut ethabi::encode(&max_fee_per_gas.into_tokens()));
        raw_tx.append(&mut ethabi::encode(&max_priority_fee_per_gas.into_tokens()));
        raw_tx.append(&mut ethabi::encode(&nonce.into_tokens()));
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

    pub fn advance_block_number(&self, val: u64) -> u64 {
        let mut inner = self.inner.write().unwrap();
        inner.block_number += val;
        inner.block_number
    }

    pub fn with_fee_history(self, history: Vec<u64>) -> Self {
        Self {
            base_fee_history: history,
            ..self
        }
    }

    pub fn with_excess_blob_gas_history(self, history: Vec<u64>) -> Self {
        Self {
            excess_blob_gas_history: history,
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
        Ok(self.inner.read().unwrap().tx_statuses.get(&hash).cloned())
    }

    async fn block_number(&self, _: &'static str) -> Result<U64, Error> {
        Ok(self.inner.read().unwrap().block_number.into())
    }

    async fn send_raw_tx(&self, tx: RawTransactionBytes) -> Result<H256, Error> {
        let mock_tx = MockTx::from(tx.0);
        let mock_tx_hash = mock_tx.hash;
        let mut inner = self.inner.write().unwrap();

        if mock_tx.nonce < inner.current_nonce {
            return Err(Error::EthereumGateway(Web3Error::Rpc(RpcError {
                message: "transaction with the same nonce already processed".to_string(),
                code: 101.into(),
                data: None,
            })));
        }

        if mock_tx.nonce == inner.pending_nonce {
            inner.pending_nonce += 1;
        }
        inner.sent_txs.insert(mock_tx_hash, mock_tx);
        Ok(mock_tx_hash)
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
        let start_block = from_block.saturating_sub(block_count - 1);
        Ok(self.base_fee_history[start_block..=from_block].to_vec())
    }

    async fn get_pending_block_base_fee_per_gas(
        &self,
        _component: &'static str,
    ) -> Result<U256, Error> {
        Ok(U256::from(*self.base_fee_history.last().unwrap()))
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

    async fn call_contract_function(
        &self,
        call: ContractCall,
    ) -> Result<Vec<ethabi::Token>, Error> {
        use ethabi::Token;

        if call.contract_address == self.multicall_address {
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
            return Ok(vec![token]);
        }
        Ok(vec![])
    }

    async fn get_tx(
        &self,
        hash: H256,
        _component: &'static str,
    ) -> Result<Option<Transaction>, Error> {
        let txs = &self.inner.read().unwrap().sent_txs;
        let Some(tx) = txs.get(&hash) else {
            return Ok(None);
        };
        Ok(Some(tx.clone().into()))
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
        block_id: BlockId,
        _component: &'static str,
    ) -> Result<Option<Block<H256>>, Error> {
        match block_id {
            BlockId::Number(BlockNumber::Number(number)) => {
                let excess_blob_gas = self
                    .excess_blob_gas_history
                    .get(number.as_usize())
                    .map(|excess_blob_gas| (*excess_blob_gas).into());
                let base_fee_per_gas = self
                    .base_fee_history
                    .get(number.as_usize())
                    .map(|base_fee| (*base_fee).into());

                Ok(Some(Block {
                    number: Some(number),
                    excess_blob_gas,
                    base_fee_per_gas,
                    ..Default::default()
                }))
            }
            _ => unimplemented!("Not needed right now"),
        }
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
            let inner = self.inner.read().unwrap();
            let mut nonce_range = inner.nonces.range(..=block_number.as_u64());
            let (_, &nonce) = nonce_range.next_back().unwrap_or((&0, &0));
            Ok(nonce.into())
        } else {
            panic!("MockEthereum::nonce_at called with non-number block tag");
        }
    }

    async fn pending_nonce(&self, _: &'static str) -> Result<U256, Error> {
        Ok(self.inner.read().unwrap().pending_nonce.into())
    }

    async fn current_nonce(&self, _: &'static str) -> Result<U256, Error> {
        Ok(self.inner.read().unwrap().current_nonce.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn managing_block_number() {
        let client = MockEthereum::default();
        let block_number = client.block_number("test").await.unwrap();
        assert_eq!(block_number, 0.into());

        client.advance_block_number(5);
        let block_number = client.block_number("test").await.unwrap();
        assert_eq!(block_number, 5.into());
    }

    #[tokio::test]
    async fn managing_transactions() {
        let client = MockEthereum::default().with_non_ordering_confirmation(true);
        client.advance_block_number(2);

        let signed_tx = client
            .sign_prepared_tx(
                b"test".to_vec(),
                Options {
                    nonce: Some(1.into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(signed_tx.nonce, 1.into());
        assert!(signed_tx.max_priority_fee_per_gas > 0.into());
        assert!(signed_tx.max_fee_per_gas > 0.into());

        let tx_hash = client.send_raw_tx(signed_tx.raw_tx.clone()).await.unwrap();
        assert_eq!(tx_hash, signed_tx.hash);

        client.execute_tx(tx_hash, true, 3);
        let returned_tx = client
            .get_tx(tx_hash, "test")
            .await
            .unwrap()
            .expect("no transaction");
        assert_eq!(returned_tx.hash, tx_hash);
        assert_eq!(returned_tx.input.0, b"test");
        assert_eq!(returned_tx.nonce, 1.into());
        assert!(returned_tx.max_priority_fee_per_gas.is_some());
        assert!(returned_tx.max_fee_per_gas.is_some());

        let tx_status = client
            .get_tx_status(tx_hash, "test")
            .await
            .unwrap()
            .expect("no transaction status");
        assert!(tx_status.success);
        assert_eq!(tx_status.tx_hash, tx_hash);
        assert_eq!(tx_status.receipt.block_number, Some(2.into()));
    }
}
