use std::sync::Mutex;

use assert_matches::assert_matches;
use async_trait::async_trait;

use zksync_config::configs::{chain::CircuitBreakerConfig, ContractsConfig};
use zksync_eth_client::{
    types::{Error, ExecutedTxStatus, FailureInfo, SignedCallResult},
    BoundEthInterface, EthInterface,
};
use zksync_types::web3::types::Block;
use zksync_types::{
    ethabi::Token,
    web3::{
        self,
        contract::{
            tokens::{Detokenize, Tokenize},
            Options,
        },
        error::TransportError,
        ethabi,
        types::{
            Address, BlockId, BlockNumber, Filter, Log, Transaction, TransactionReceipt, H160,
            H256, U256,
        },
    },
    L1ChainId, U64,
};

#[derive(Debug)]
pub struct ETHDirectClientMock {
    contract: ethabi::Contract,
    // next 2 are needed for simulation of the few ZksInterface functions,
    // to test retries
    circuit_breaker_config: CircuitBreakerConfig,
    counter: Mutex<u8>,
}

impl ETHDirectClientMock {
    pub fn new() -> Self {
        Self {
            contract: Default::default(),
            circuit_breaker_config: get_test_circuit_breaker_config(),
            counter: Mutex::new(0),
        }
    }

    fn inc_counter(&self) {
        let mut current = self.counter.lock().unwrap();
        *current += 1;
    }

    fn get_counter_cur_val(&self) -> u8 {
        let current = self.counter.lock().unwrap();
        *current
    }

    fn reset_counter(&self) {
        let mut current = self.counter.lock().unwrap();
        *current = 0;
    }

    // The idea of this function is to simulate the behavior when function call fails all the time,
    // and  when the current attempt is the last one it succeeds and returns Ok()
    pub fn simulate_get_contract_behavior<R>(&self) -> Result<R, Error>
    where
        R: Detokenize + Unpin,
    {
        self.inc_counter();

        let cur_val = self.get_counter_cur_val();
        // If the condition returns `true`, it means that its the last attempt of the retry() wrapper function.
        // Otherwise we pretend that there are some eth_client issues and return Err()
        if cur_val as usize == self.circuit_breaker_config.http_req_max_retry_number {
            self.reset_counter();
            Ok(
                Detokenize::from_tokens(vec![Token::Array(vec![Token::Tuple(vec![
                    Token::Address(H160::zero()),
                    Token::Array(vec![Token::FixedBytes(vec![0, 0, 0, 0, 0, 0])]),
                ])])])
                .unwrap(),
            )
        } else {
            Err(Error::EthereumGateway(web3::error::Error::Transport(
                TransportError::Code(503),
            )))
        }
    }
}

fn get_test_circuit_breaker_config() -> CircuitBreakerConfig {
    CircuitBreakerConfig {
        sync_interval_ms: 1000,
        http_req_max_retry_number: 5,
        http_req_retry_interval_sec: 2,
    }
}
#[async_trait]
impl EthInterface for ETHDirectClientMock {
    /// Note: The only really implemented method! Other ones are just stubs.
    #[allow(clippy::too_many_arguments)]
    async fn call_contract_function<R, A, B, P>(
        &self,
        _func: &str,
        _params: P,
        _from: A,
        _options: Options,
        _block: B,
        _contract_address: Address,
        _contract_abi: ethabi::Contract,
    ) -> Result<R, Error>
    where
        R: Detokenize + Unpin,
        A: Into<Option<Address>> + Send,
        B: Into<Option<BlockId>> + Send,
        P: Tokenize + Send,
    {
        self.simulate_get_contract_behavior()
    }

    async fn get_tx_status(
        &self,
        _hash: H256,
        _: &'static str,
    ) -> Result<Option<ExecutedTxStatus>, Error> {
        Ok(None)
    }

    async fn block_number(&self, _: &'static str) -> Result<U64, Error> {
        Ok(Default::default())
    }

    async fn send_raw_tx(&self, _tx: Vec<u8>) -> Result<H256, Error> {
        Ok(Default::default())
    }

    async fn nonce_at_for_account(
        &self,
        _account: Address,
        _block: BlockNumber,
        _: &'static str,
    ) -> Result<U256, Error> {
        Ok(Default::default())
    }

    async fn get_gas_price(&self, _: &'static str) -> Result<U256, Error> {
        Ok(Default::default())
    }

    async fn base_fee_history(
        &self,
        _from_block: usize,
        _block_count: usize,
        _component: &'static str,
    ) -> Result<Vec<u64>, Error> {
        Ok(Default::default())
    }

    async fn get_pending_block_base_fee_per_gas(
        &self,
        _component: &'static str,
    ) -> Result<U256, Error> {
        Ok(Default::default())
    }

    async fn failure_reason(&self, _tx_hash: H256) -> Result<Option<FailureInfo>, Error> {
        Ok(Default::default())
    }

    async fn get_tx(
        &self,
        _hash: H256,
        _component: &'static str,
    ) -> Result<Option<Transaction>, Error> {
        Ok(Default::default())
    }

    async fn tx_receipt(
        &self,
        _tx_hash: H256,
        _component: &'static str,
    ) -> Result<Option<TransactionReceipt>, Error> {
        Ok(Default::default())
    }

    async fn eth_balance(
        &self,
        _address: Address,
        _component: &'static str,
    ) -> Result<U256, Error> {
        Ok(Default::default())
    }

    async fn logs(&self, _filter: Filter, _component: &'static str) -> Result<Vec<Log>, Error> {
        Ok(Default::default())
    }

    async fn block(
        &self,
        _block_id: String,
        _component: &'static str,
    ) -> Result<Option<Block<H256>>, Error> {
        Ok(Default::default())
    }
}

#[async_trait]
impl BoundEthInterface for ETHDirectClientMock {
    fn contract(&self) -> &ethabi::Contract {
        &self.contract
    }

    fn contract_addr(&self) -> H160 {
        Default::default()
    }

    fn chain_id(&self) -> L1ChainId {
        L1ChainId(0)
    }

    fn sender_account(&self) -> Address {
        Default::default()
    }

    async fn sign_prepared_tx_for_addr(
        &self,
        _data: Vec<u8>,
        _contract_addr: H160,
        _options: Options,
        _component: &'static str,
    ) -> Result<SignedCallResult, Error> {
        Ok(SignedCallResult {
            raw_tx: vec![],
            max_priority_fee_per_gas: U256::zero(),
            max_fee_per_gas: U256::zero(),
            nonce: U256::zero(),
            hash: H256::zero(),
        })
    }

    async fn allowance_on_account(
        &self,
        _token_address: Address,
        _contract_address: Address,
        _erc20_abi: ethabi::Contract,
    ) -> Result<U256, Error> {
        Ok(Default::default())
    }
}

#[tokio::test]
async fn retries_for_facet_selectors() {
    let eth_client = ETHDirectClientMock::new();

    let result: Result<Token, Error> = eth_client
        .call_contract_function(
            "get_verification_key",
            (),
            None,
            Default::default(),
            None,
            Address::default(),
            eth_client.contract().clone(),
        )
        .await;

    assert_matches!(
        result,
        Err(Error::EthereumGateway(web3::error::Error::Transport(
            TransportError::Code(503),
        )))
    );

    let contracts = ContractsConfig::from_env();
    let config = get_test_circuit_breaker_config();
    let facet_selectors_checker = crate::facet_selectors::FacetSelectorsChecker::new(
        &config,
        eth_client,
        contracts.diamond_proxy_addr,
    );

    assert_matches!(
        facet_selectors_checker.get_facets_token_with_retry().await,
        Ok(_)
    );
}
