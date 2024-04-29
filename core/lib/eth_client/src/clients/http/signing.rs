use std::{fmt, sync::Arc};

use async_trait::async_trait;
use zksync_config::{configs::ContractsConfig, EthConfig};
use zksync_contracts::hyperchain_contract;
use zksync_eth_signer::{raw_ethereum_tx::TransactionParameters, EthereumSigner, PrivateKeySigner};
use zksync_types::{
    web3::{
        self,
        contract::tokens::Detokenize,
        ethabi,
        transports::Http,
        types::{
            Address, BlockId, BlockNumber, Filter, Log, Transaction, TransactionReceipt, H160,
            H256, U256, U64,
        },
    },
    K256PrivateKey, L1ChainId, EIP_4844_TX_TYPE,
};

use super::{query::QueryClient, Method, LATENCIES};
use crate::{
    types::{encode_blob_tx_with_sidecar, Error, ExecutedTxStatus, FailureInfo, SignedCallResult},
    Block, BoundEthInterface, CallFunctionArgs, ContractCall, EthInterface, Options,
    RawTransactionBytes,
};

/// HTTP-based Ethereum client, backed by a private key to sign transactions.
pub type PKSigningClient = SigningClient<PrivateKeySigner>;

impl PKSigningClient {
    pub fn from_config(
        eth_sender: &EthConfig,
        contracts_config: &ContractsConfig,
        l1_chain_id: L1ChainId,
        operator_private_key: K256PrivateKey,
    ) -> Self {
        Self::from_config_inner(
            eth_sender,
            contracts_config,
            l1_chain_id,
            operator_private_key,
        )
    }

    pub fn new_raw(
        operator_private_key: K256PrivateKey,
        diamond_proxy_addr: Address,
        default_priority_fee_per_gas: u64,
        l1_chain_id: L1ChainId,
        web3_url: &str,
    ) -> Self {
        let transport = Http::new(web3_url).expect("Failed to create transport");
        let operator_address = operator_private_key.address();
        let signer = PrivateKeySigner::new(operator_private_key);
        tracing::info!("Operator address: {:?}", operator_address);
        SigningClient::new(
            transport,
            hyperchain_contract(),
            operator_address,
            signer,
            diamond_proxy_addr,
            default_priority_fee_per_gas.into(),
            l1_chain_id,
        )
    }

    fn from_config_inner(
        eth_sender: &EthConfig,
        contracts_config: &ContractsConfig,
        l1_chain_id: L1ChainId,
        operator_private_key: K256PrivateKey,
    ) -> Self {
        let diamond_proxy_addr = contracts_config.diamond_proxy_addr;
        let default_priority_fee_per_gas = eth_sender
            .gas_adjuster
            .expect("Gas adjuster")
            .default_priority_fee_per_gas;
        let main_node_url = &eth_sender.web3_url;

        SigningClient::new_raw(
            operator_private_key,
            diamond_proxy_addr,
            default_priority_fee_per_gas,
            l1_chain_id,
            main_node_url,
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

    async fn send_raw_tx(&self, tx: RawTransactionBytes) -> Result<H256, Error> {
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

    async fn call_contract_function(
        &self,
        call: ContractCall,
    ) -> Result<Vec<ethabi::Token>, Error> {
        self.query_client.call_contract_function(call).await
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
        block_id: BlockId,
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

        if options.transaction_type == Some(EIP_4844_TX_TYPE.into()) {
            if options.max_fee_per_blob_gas.is_none() {
                return Err(Error::Eip4844MissingMaxFeePerBlobGas);
            }
            if options.blob_versioned_hashes.is_none() {
                return Err(Error::Eip4844MissingBlobVersionedHashes);
            }
        }

        // Fetch current base fee and add `max_priority_fee_per_gas`
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
            transaction_type: options.transaction_type,
            access_list: None,
            max_fee_per_gas,
            max_fee_per_blob_gas: options.max_fee_per_blob_gas,
            blob_versioned_hashes: options.blob_versioned_hashes,
        };

        let mut signed_tx = self.inner.eth_signer.sign_transaction(tx).await?;
        let hash = web3::signing::keccak256(&signed_tx).into();
        latency.observe();

        if let Some(sidecar) = options.blob_tx_sidecar {
            signed_tx = encode_blob_tx_with_sidecar(&signed_tx, &sidecar);
        }

        Ok(SignedCallResult::new(
            RawTransactionBytes(signed_tx),
            max_priority_fee_per_gas,
            max_fee_per_gas,
            nonce,
            hash,
        ))
    }

    async fn allowance_on_account(
        &self,
        token_address: Address,
        address: Address,
        erc20_abi: ethabi::Contract,
    ) -> Result<U256, Error> {
        let latency = LATENCIES.direct[&Method::Allowance].start();
        let args = CallFunctionArgs::new("allowance", (self.inner.sender_account, address))
            .for_contract(token_address, erc20_abi);
        let res = self.call_contract_function(args).await?;
        latency.observe();
        Ok(U256::from_tokens(res)?)
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
