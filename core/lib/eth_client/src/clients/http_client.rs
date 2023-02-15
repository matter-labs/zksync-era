// Built-in deps
use std::cmp::min;
use std::sync::Arc;
use std::{fmt, time::Instant};

use async_trait::async_trait;
use zksync_config::ZkSyncConfig;
use zksync_contracts::zksync_contract;
use zksync_eth_signer::PrivateKeySigner;
// External uses
use zksync_types::web3::{
    self,
    contract::{
        tokens::{Detokenize, Tokenize},
        Contract, Options,
    },
    ethabi,
    transports::Http,
    types::{
        Address, BlockId, BlockNumber, Bytes, Filter, Log, Transaction, TransactionId,
        TransactionReceipt, H160, H256, U256, U64,
    },
    Web3,
};
use zksync_types::{L1ChainId, PackedEthSignature, EIP_1559_TX_TYPE};

// Workspace uses
use zksync_eth_signer::{raw_ethereum_tx::TransactionParameters, EthereumSigner};

pub type EthereumClient = ETHDirectClient<PrivateKeySigner>;

/// Gas limit value to be used in transaction if for some reason
/// gas limit was not set for it.
///
/// This is an emergency value, which will not be used normally.
const FALLBACK_GAS_LIMIT: u64 = 3_000_000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Request to ethereum gateway failed: {0}")]
    EthereumGateway(#[from] zksync_types::web3::Error),
    #[error("Call to contract failed: {0}")]
    Contract(#[from] zksync_types::web3::contract::Error),
    #[error("Transaction signing failed: {0}")]
    Signer(#[from] zksync_eth_signer::error::SignerError),
    #[error("Decoding revert reason failed: {0}")]
    Decode(#[from] ethabi::Error),
    #[error("Max fee {0} less than priority fee {1}")]
    WrongFeeProvided(U256, U256),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignedCallResult {
    pub raw_tx: Vec<u8>,
    pub max_priority_fee_per_gas: U256,
    pub max_fee_per_gas: U256,
    pub nonce: U256,
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
#[derive(Debug, Clone)]
pub struct FailureInfo {
    pub revert_code: i64,
    pub revert_reason: String,
    pub gas_used: Option<U256>,
    pub gas_limit: U256,
}

#[async_trait]
pub trait EthInterface {
    async fn nonce_at(&self, block: BlockNumber, component: &'static str) -> Result<U256, Error>;
    async fn current_nonce(&self, component: &'static str) -> Result<U256, Error> {
        self.nonce_at(BlockNumber::Latest, component).await
    }
    async fn pending_nonce(&self, component: &'static str) -> Result<U256, Error> {
        self.nonce_at(BlockNumber::Pending, component).await
    }
    async fn base_fee_history(
        &self,
        from_block: usize,
        block_count: usize,
        component: &'static str,
    ) -> Result<Vec<u64>, Error>;
    async fn get_gas_price(&self, component: &'static str) -> Result<U256, Error>;
    async fn block_number(&self, component: &'static str) -> Result<U64, Error>;
    async fn send_raw_tx(&self, tx: Vec<u8>) -> Result<H256, Error>;
    async fn sign_prepared_tx_for_addr(
        &self,
        data: Vec<u8>,
        contract_addr: H160,
        options: Options,
        component: &'static str,
    ) -> Result<SignedCallResult, Error>;
    async fn get_tx_status(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<ExecutedTxStatus>, Error>;
    async fn failure_reason(&self, tx_hash: H256) -> Result<Option<FailureInfo>, Error>;
}

struct ETHDirectClientInner<S: EthereumSigner> {
    eth_signer: S,
    sender_account: Address,
    contract_addr: H160,
    contract: ethabi::Contract,
    chain_id: L1ChainId,
    default_priority_fee_per_gas: U256,
    web3: Web3<Http>,
}

#[derive(Clone)]
pub struct ETHDirectClient<S: EthereumSigner> {
    inner: Arc<ETHDirectClientInner<S>>,
}

impl<S: EthereumSigner> fmt::Debug for ETHDirectClient<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We do not want to have a private key in the debug representation.

        f.debug_struct("ETHDirectClient")
            .field("sender_account", &self.inner.sender_account)
            .field("contract_addr", &self.inner.contract_addr)
            .field("chain_id", &self.inner.chain_id)
            .finish()
    }
}

#[async_trait]
impl<S: EthereumSigner> EthInterface for ETHDirectClient<S> {
    async fn nonce_at(&self, block: BlockNumber, component: &'static str) -> Result<U256, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "nonce_at");
        let start = Instant::now();
        let nonce = self
            .inner
            .web3
            .eth()
            .transaction_count(self.inner.sender_account, Some(block))
            .await?;
        metrics::histogram!("eth_client.direct.current_nonce", start.elapsed());
        Ok(nonce)
    }

    async fn block_number(&self, component: &'static str) -> Result<U64, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "block_number");
        let start = Instant::now();
        let block_number = self.inner.web3.eth().block_number().await?;
        metrics::histogram!("eth_client.direct.block_number", start.elapsed());
        Ok(block_number)
    }

    async fn get_gas_price(&self, component: &'static str) -> Result<U256, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "get_gas_price");
        let start = Instant::now();
        let network_gas_price = self.inner.web3.eth().gas_price().await?;
        metrics::histogram!("eth_client.direct.get_gas_price", start.elapsed());
        Ok(network_gas_price)
    }

    async fn sign_prepared_tx_for_addr(
        &self,
        data: Vec<u8>,
        contract_addr: H160,
        options: Options,
        component: &'static str,
    ) -> Result<SignedCallResult, Error> {
        let start = Instant::now();

        // fetch current max priority fee per gas
        let max_priority_fee_per_gas = match options.max_priority_fee_per_gas {
            Some(max_priority_fee_per_gas) => max_priority_fee_per_gas,
            None => self.inner.default_priority_fee_per_gas,
        };

        // fetch current base fee and add max_priority_fee_per_gas
        let max_fee_per_gas = match options.max_fee_per_gas {
            Some(max_fee_per_gas) => max_fee_per_gas,
            None => {
                self.get_pending_block_base_fee_per_gas(component).await? + max_priority_fee_per_gas
            }
        };

        if max_fee_per_gas < max_priority_fee_per_gas {
            return Err(Error::WrongFeeProvided(
                max_fee_per_gas,
                max_priority_fee_per_gas,
            ));
        }

        let nonce = match options.nonce {
            Some(nonce) => nonce,
            None => self.pending_nonce(component).await?,
        };

        let gas = match options.gas {
            Some(gas) => gas,
            None => {
                // Verbosity level is set to `error`, since we expect all the transactions to have
                // a set limit, but don't want to crаsh the application if for some reason in some
                // place limit was not set.
                vlog::error!(
                    "No gas limit was set for transaction, using the default limit: {}",
                    FALLBACK_GAS_LIMIT
                );

                U256::from(FALLBACK_GAS_LIMIT)
            }
        };

        let tx = TransactionParameters {
            nonce,
            to: Some(contract_addr),
            gas,
            value: options.value.unwrap_or_default(),
            data,
            chain_id: self.inner.chain_id.0 as u64,
            max_priority_fee_per_gas,
            gas_price: None,
            transaction_type: Some(EIP_1559_TX_TYPE.into()),
            access_list: None,
            max_fee_per_gas,
        };

        let signed_tx = self.inner.eth_signer.sign_transaction(tx).await?;
        let hash = self
            .inner
            .web3
            .web3()
            .sha3(Bytes(signed_tx.clone()))
            .await?;

        metrics::histogram!(
            "eth_client.direct.sign_prepared_tx_for_addr",
            start.elapsed()
        );
        Ok(SignedCallResult {
            raw_tx: signed_tx,
            max_priority_fee_per_gas,
            max_fee_per_gas,
            nonce,
            hash,
        })
    }

    async fn send_raw_tx(&self, tx: Vec<u8>) -> Result<H256, Error> {
        let start = Instant::now();
        let tx = self
            .inner
            .web3
            .eth()
            .send_raw_transaction(Bytes(tx))
            .await?;
        metrics::histogram!("eth_client.direct.send_raw_tx", start.elapsed());
        Ok(tx)
    }

    async fn base_fee_history(
        &self,
        upto_block: usize,
        block_count: usize,
        component: &'static str,
    ) -> Result<Vec<u64>, Error> {
        const MAX_REQUEST_CHUNK: usize = 1024;

        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "base_fee_history");
        let start = Instant::now();

        let mut history = Vec::with_capacity(block_count);
        let from_block = upto_block.saturating_sub(block_count);

        // Here we are requesting fee_history from blocks
        // (from_block; upto_block] in chunks of size MAX_REQUEST_CHUNK
        // starting from the oldest block.
        for chunk_start in (from_block..=upto_block).step_by(MAX_REQUEST_CHUNK) {
            let chunk_end = (chunk_start + MAX_REQUEST_CHUNK).min(upto_block);
            let chunk_size = chunk_end - chunk_start;
            let chunk = self
                .inner
                .web3
                .eth()
                .fee_history(chunk_size.into(), chunk_end.into(), None)
                .await?
                .base_fee_per_gas;

            history.extend(chunk);
        }

        metrics::histogram!("eth_client.direct.base_fee", start.elapsed());
        Ok(history.into_iter().map(|fee| fee.as_u64()).collect())
    }
    async fn get_tx_status(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<ExecutedTxStatus>, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "get_tx_status");
        let start = Instant::now();

        let receipt = self.tx_receipt(hash, component).await?;
        let res = match receipt {
            Some(receipt) => match (receipt.status, receipt.block_number) {
                (Some(status), Some(_)) => {
                    let success = status.as_u64() == 1;

                    Some(ExecutedTxStatus {
                        tx_hash: receipt.transaction_hash,
                        success,
                        receipt,
                    })
                }
                _ => None,
            },
            _ => None,
        };
        metrics::histogram!("eth_client.direct.get_tx_status", start.elapsed());
        Ok(res)
    }

    async fn failure_reason(&self, tx_hash: H256) -> Result<Option<FailureInfo>, Error> {
        let start = Instant::now();
        let transaction = self.inner.web3.eth().transaction(tx_hash.into()).await?;
        let receipt = self.inner.web3.eth().transaction_receipt(tx_hash).await?;

        match (transaction, receipt) {
            (Some(transaction), Some(receipt)) => {
                let gas_limit = transaction.gas;
                let gas_used = receipt.gas_used;

                let call_request = web3::types::CallRequest {
                    from: transaction.from,
                    to: transaction.to,
                    gas: Some(transaction.gas),
                    gas_price: transaction.gas_price,
                    max_fee_per_gas: None,
                    max_priority_fee_per_gas: None,
                    value: Some(transaction.value),
                    data: Some(transaction.input),
                    transaction_type: None,
                    access_list: None,
                };

                let call_error = self
                    .inner
                    .web3
                    .eth()
                    .call(call_request, receipt.block_number.map(Into::into))
                    .await
                    .err();

                let failure_info = match call_error {
                    Some(web3::Error::Rpc(rpc_error)) => {
                        let revert_code = rpc_error.code.code();
                        let message_len =
                            min("execution reverted: ".len(), rpc_error.message.len());
                        let revert_reason = rpc_error.message[message_len..].to_string();

                        Ok(Some(FailureInfo {
                            revert_code,
                            revert_reason,
                            gas_used,
                            gas_limit,
                        }))
                    }
                    Some(err) => Err(err.into()),
                    None => Ok(None),
                };

                metrics::histogram!("eth_client.direct.failure_reason", start.elapsed());

                failure_info
            }
            _ => Ok(None),
        }
    }
}

impl<S: EthereumSigner> ETHDirectClient<S> {
    pub fn new(
        transport: Http,
        contract: ethabi::Contract,
        operator_eth_addr: H160,
        eth_signer: S,
        contract_eth_addr: H160,
        default_priority_fee_per_gas: U256,
        chain_id: L1ChainId,
    ) -> Self {
        Self {
            inner: Arc::new(ETHDirectClientInner {
                sender_account: operator_eth_addr,
                eth_signer,
                contract_addr: contract_eth_addr,
                chain_id,
                contract,
                default_priority_fee_per_gas,
                web3: Web3::new(transport),
            }),
        }
    }

    pub async fn get_pending_block_base_fee_per_gas(
        &self,
        component: &'static str,
    ) -> Result<U256, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "get_pending_block_base_fee_per_gas");
        let start = Instant::now();
        let block = self
            .inner
            .web3
            .eth()
            .block(BlockId::Number(BlockNumber::Pending))
            .await?
            .unwrap(); // Pending block should always exist

        metrics::histogram!("eth_client.direct.base_fee", start.elapsed());
        // base_fee_per_gas always exists after London fork
        Ok(block.base_fee_per_gas.unwrap())
    }

    pub fn main_contract_with_address(&self, address: Address) -> Contract<Http> {
        Contract::new(self.inner.web3.eth(), address, self.inner.contract.clone())
    }

    pub fn main_contract(&self) -> Contract<Http> {
        self.main_contract_with_address(self.inner.contract_addr)
    }

    pub fn create_contract(&self, address: Address, contract: ethabi::Contract) -> Contract<Http> {
        Contract::new(self.inner.web3.eth(), address, contract)
    }

    pub async fn block(&self, id: BlockId) -> Result<Option<web3::types::Block<H256>>, Error> {
        let start = Instant::now();
        let block = self.inner.web3.eth().block(id).await?;
        metrics::histogram!("eth_client.direct.block", start.elapsed());
        Ok(block)
    }

    pub async fn sign_prepared_tx(
        &self,
        data: Vec<u8>,
        options: Options,
        component: &'static str,
    ) -> Result<SignedCallResult, Error> {
        self.sign_prepared_tx_for_addr(data, self.inner.contract_addr, options, component)
            .await
    }

    pub async fn tx_receipt(
        &self,
        tx_hash: H256,
        component: &'static str,
    ) -> Result<Option<TransactionReceipt>, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "tx_receipt");
        let start = Instant::now();
        let receipt = self.inner.web3.eth().transaction_receipt(tx_hash).await?;
        metrics::histogram!("eth_client.direct.tx_receipt", start.elapsed());
        Ok(receipt)
    }

    pub async fn eth_balance(
        &self,
        address: Address,
        component: &'static str,
    ) -> Result<U256, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "eth_balance");
        let start = Instant::now();
        let balance = self.inner.web3.eth().balance(address, None).await?;
        metrics::histogram!("eth_client.direct.eth_balance", start.elapsed());
        Ok(balance)
    }

    pub async fn sender_eth_balance(&self, component: &'static str) -> Result<U256, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "sender_eth_balance");
        self.eth_balance(self.inner.sender_account, component).await
    }

    pub async fn allowance(
        &self,
        token_address: Address,
        erc20_abi: ethabi::Contract,
    ) -> Result<U256, Error> {
        self.allowance_on_contract(token_address, self.inner.contract_addr, erc20_abi)
            .await
    }

    pub async fn allowance_on_contract(
        &self,
        token_address: Address,
        contract_address: Address,
        erc20_abi: ethabi::Contract,
    ) -> Result<U256, Error> {
        let start = Instant::now();
        let res = self
            .call_contract_function(
                "allowance",
                (self.inner.sender_account, contract_address),
                None,
                Options::default(),
                None,
                token_address,
                erc20_abi,
            )
            .await?;
        metrics::histogram!("eth_client.direct.allowance", start.elapsed());
        Ok(res)
    }

    pub async fn call_main_contract_function<R, A, P, B>(
        &self,
        func: &str,
        params: P,
        from: A,
        options: Options,
        block: B,
    ) -> Result<R, Error>
    where
        R: Detokenize + Unpin,
        A: Into<Option<Address>>,
        B: Into<Option<BlockId>>,
        P: Tokenize,
    {
        self.call_contract_function(
            func,
            params,
            from,
            options,
            block,
            self.inner.contract_addr,
            self.inner.contract.clone(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn call_contract_function<R, A, B, P>(
        &self,
        func: &str,
        params: P,
        from: A,
        options: Options,
        block: B,
        token_address: Address,
        erc20_abi: ethabi::Contract,
    ) -> Result<R, Error>
    where
        R: Detokenize + Unpin,
        A: Into<Option<Address>>,
        B: Into<Option<BlockId>>,
        P: Tokenize,
    {
        let start = Instant::now();
        let contract = Contract::new(self.inner.web3.eth(), token_address, erc20_abi);
        let res = contract.query(func, params, from, options, block).await?;
        metrics::histogram!("eth_client.direct.call_contract_function", start.elapsed());
        Ok(res)
    }

    pub async fn logs(&self, filter: Filter, component: &'static str) -> Result<Vec<Log>, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "logs");
        let start = Instant::now();
        let logs = self.inner.web3.eth().logs(filter).await?;
        metrics::histogram!("eth_client.direct.logs", start.elapsed());
        Ok(logs)
    }

    pub fn contract(&self) -> &ethabi::Contract {
        &self.inner.contract
    }

    pub fn contract_addr(&self) -> H160 {
        self.inner.contract_addr
    }

    pub fn chain_id(&self) -> L1ChainId {
        self.inner.chain_id
    }

    pub fn sender_account(&self) -> Address {
        self.inner.sender_account
    }

    pub fn encode_tx_data<P: Tokenize>(&self, func: &str, params: P) -> Vec<u8> {
        let f = self
            .contract()
            .function(func)
            .expect("failed to get function parameters");

        f.encode_input(&params.into_tokens())
            .expect("failed to encode parameters")
    }

    pub fn get_web3_transport(&self) -> &Http {
        self.inner.web3.transport()
    }

    pub async fn get_tx(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<Transaction>, Error> {
        metrics::counter!("server.ethereum_gateway.call", 1, "component" => component, "method" => "get_tx");
        let tx = self
            .inner
            .web3
            .eth()
            .transaction(TransactionId::Hash(hash))
            .await?;
        Ok(tx)
    }
}

impl EthereumClient {
    pub fn from_config(config: &ZkSyncConfig) -> Self {
        let transport = web3::transports::Http::new(&config.eth_client.web3_url).unwrap();

        let operator_address = PackedEthSignature::address_from_private_key(
            &config.eth_sender.sender.operator_private_key,
        )
        .expect("Failed to get address from private key");

        vlog::info!("Operator address: {:?}", operator_address);

        ETHDirectClient::new(
            transport,
            zksync_contract(),
            config.eth_sender.sender.operator_commit_eth_addr,
            PrivateKeySigner::new(config.eth_sender.sender.operator_private_key),
            config.contracts.diamond_proxy_addr,
            config
                .eth_sender
                .gas_adjuster
                .default_priority_fee_per_gas
                .into(),
            L1ChainId(config.eth_client.chain_id),
        )
    }
}
