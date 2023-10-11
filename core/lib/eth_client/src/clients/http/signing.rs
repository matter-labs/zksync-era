use async_trait::async_trait;

use std::{fmt, sync::Arc};

use zksync_config::{ContractsConfig, ETHClientConfig, ETHSenderConfig};
use zksync_contracts::zksync_contract;
use zksync_eth_signer::{raw_ethereum_tx::TransactionParameters, EthereumSigner, PrivateKeySigner};
use zksync_types::web3::{
    self,
    contract::{
        tokens::{Detokenize, Tokenize},
        Options,
    },
    ethabi,
    transports::Http,
    types::{
        Address, Block, BlockId, BlockNumber, Filter, Log, Transaction, TransactionReceipt, H160,
        H256, U256, U64,
    },
};
use zksync_types::{L1ChainId, PackedEthSignature, EIP_1559_TX_TYPE};

use super::{query::QueryClient, Method, LATENCIES};
use crate::{
    types::{Error, ExecutedTxStatus, FailureInfo, SignedCallResult},
    BoundEthInterface, EthInterface,
};

/// HTTP-based Ethereum client, backed by a private key to sign transactions.
pub type PKSigningClient = SigningClient<PrivateKeySigner>;

impl PKSigningClient {
    pub fn from_config(
        eth_sender: &ETHSenderConfig,
        contracts_config: &ContractsConfig,
        eth_client: &ETHClientConfig,
    ) -> Self {
        // Gather required data from the config.
        // It's done explicitly to simplify getting rid of this function later.
        let main_node_url = &eth_client.web3_url;
        let operator_private_key = eth_sender
            .sender
            .private_key()
            .expect("Operator private key is required for signing client");
        let diamond_proxy_addr = contracts_config.diamond_proxy_addr;
        let default_priority_fee_per_gas = eth_sender.gas_adjuster.default_priority_fee_per_gas;
        let l1_chain_id = eth_client.chain_id;

        let transport =
            web3::transports::Http::new(main_node_url).expect("Failed to create transport");
        let operator_address = PackedEthSignature::address_from_private_key(&operator_private_key)
            .expect("Failed to get address from private key");

        tracing::info!("Operator address: {:?}", operator_address);

        SigningClient::new(
            transport,
            zksync_contract(),
            operator_address,
            PrivateKeySigner::new(operator_private_key),
            diamond_proxy_addr,
            default_priority_fee_per_gas.into(),
            L1ChainId(l1_chain_id),
        )
    }
}

/// Gas limit value to be used in transaction if for some reason
/// gas limit was not set for it.
///
/// This is an emergency value, which will not be used normally.
const FALLBACK_GAS_LIMIT: u64 = 3_000_000;

/// HTTP-based client, instantiated for a certain account.
/// This client is capable of signing transactions.
#[derive(Clone)]
pub struct SigningClient<S: EthereumSigner> {
    inner: Arc<ETHDirectClientInner<S>>,
    query_client: QueryClient,
}

struct ETHDirectClientInner<S: EthereumSigner> {
    eth_signer: S,
    sender_account: Address,
    contract_addr: H160,
    contract: ethabi::Contract,
    chain_id: L1ChainId,
    default_priority_fee_per_gas: U256,
}

impl<S: EthereumSigner> fmt::Debug for SigningClient<S> {
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
impl<S: EthereumSigner> EthInterface for SigningClient<S> {
    async fn nonce_at_for_account(
        &self,
        account: Address,
        block: BlockNumber,
        component: &'static str,
    ) -> Result<U256, Error> {
        self.query_client
            .nonce_at_for_account(account, block, component)
            .await
    }

    async fn block_number(&self, component: &'static str) -> Result<U64, Error> {
        self.query_client.block_number(component).await
    }

    async fn get_gas_price(&self, component: &'static str) -> Result<U256, Error> {
        self.query_client.get_gas_price(component).await
    }

    async fn send_raw_tx(&self, tx: Vec<u8>) -> Result<H256, Error> {
        self.query_client.send_raw_tx(tx).await
    }

    async fn base_fee_history(
        &self,
        upto_block: usize,
        block_count: usize,
        component: &'static str,
    ) -> Result<Vec<u64>, Error> {
        self.query_client
            .base_fee_history(upto_block, block_count, component)
            .await
    }

    async fn get_pending_block_base_fee_per_gas(
        &self,
        component: &'static str,
    ) -> Result<U256, Error> {
        self.query_client
            .get_pending_block_base_fee_per_gas(component)
            .await
    }

    async fn get_tx_status(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<ExecutedTxStatus>, Error> {
        self.query_client.get_tx_status(hash, component).await
    }

    async fn failure_reason(&self, tx_hash: H256) -> Result<Option<FailureInfo>, Error> {
        self.query_client.failure_reason(tx_hash).await
    }

    async fn get_tx(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<Transaction>, Error> {
        self.query_client.get_tx(hash, component).await
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
        self.query_client
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
        self.query_client.tx_receipt(tx_hash, component).await
    }

    async fn eth_balance(&self, address: Address, component: &'static str) -> Result<U256, Error> {
        self.query_client.eth_balance(address, component).await
    }

    async fn logs(&self, filter: Filter, component: &'static str) -> Result<Vec<Log>, Error> {
        self.query_client.logs(filter, component).await
    }

    async fn block(
        &self,
        block_id: String,
        component: &'static str,
    ) -> Result<Option<Block<H256>>, Error> {
        self.query_client.block(block_id, component).await
    }
}

#[async_trait]
impl<S: EthereumSigner> BoundEthInterface for SigningClient<S> {
    fn contract(&self) -> &ethabi::Contract {
        &self.inner.contract
    }

    fn contract_addr(&self) -> H160 {
        self.inner.contract_addr
    }

    fn chain_id(&self) -> L1ChainId {
        self.inner.chain_id
    }

    fn sender_account(&self) -> Address {
        self.inner.sender_account
    }

    async fn sign_prepared_tx_for_addr(
        &self,
        data: Vec<u8>,
        contract_addr: H160,
        options: Options,
        component: &'static str,
    ) -> Result<SignedCallResult, Error> {
        let latency = LATENCIES.direct[&Method::SignPreparedTx].start();
        // Fetch current max priority fee per gas
        let max_priority_fee_per_gas = match options.max_priority_fee_per_gas {
            Some(max_priority_fee_per_gas) => max_priority_fee_per_gas,
            None => self.inner.default_priority_fee_per_gas,
        };

        // Fetch current base fee and add max_priority_fee_per_gas
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

        let gas = options.gas.unwrap_or_else(|| {
            // Verbosity level is set to `error`, since we expect all the transactions to have
            // a set limit, but don't want to crаsh the application if for some reason in some
            // place limit was not set.
            tracing::error!(
                "No gas limit was set for transaction, using the default limit: {}",
                FALLBACK_GAS_LIMIT
            );

            U256::from(FALLBACK_GAS_LIMIT)
        });

        let tx = TransactionParameters {
            nonce,
            to: Some(contract_addr),
            gas,
            value: options.value.unwrap_or_default(),
            data,
            chain_id: self.inner.chain_id.0,
            max_priority_fee_per_gas,
            gas_price: None,
            transaction_type: Some(EIP_1559_TX_TYPE.into()),
            access_list: None,
            max_fee_per_gas,
        };

        let signed_tx = self.inner.eth_signer.sign_transaction(tx).await?;
        let hash = web3::signing::keccak256(&signed_tx).into();
        latency.observe();
        Ok(SignedCallResult {
            raw_tx: signed_tx,
            max_priority_fee_per_gas,
            max_fee_per_gas,
            nonce,
            hash,
        })
    }

    async fn allowance_on_account(
        &self,
        token_address: Address,
        address: Address,
        erc20_abi: ethabi::Contract,
    ) -> Result<U256, Error> {
        let latency = LATENCIES.direct[&Method::Allowance].start();
        let res = self
            .call_contract_function(
                "allowance",
                (self.inner.sender_account, address),
                None,
                Options::default(),
                None,
                token_address,
                erc20_abi,
            )
            .await?;
        latency.observe();
        Ok(res)
    }
}

impl<S: EthereumSigner> SigningClient<S> {
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
            }),
            query_client: transport.into(),
        }
    }
}
