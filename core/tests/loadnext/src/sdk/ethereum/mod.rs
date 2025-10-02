//! Utilities for the on-chain operations, such as `Deposit` and `FullExit`.

use std::time::{Duration, Instant};

use serde_json::{Map, Value};
use zksync_eth_client::{
    clients::SigningClient, BoundEthInterface, CallFunctionArgs, ContractCallError, EthInterface,
    Options,
};
use zksync_eth_signer::EthereumSigner;
use zksync_types::{
    api::BridgeAddresses,
    ethabi,
    l1::L1Tx,
    network::Network,
    url::SensitiveUrl,
    web3::{contract::Tokenize, TransactionReceipt},
    Address, L1TxCommonData, SLChainId, H160, H256, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256,
};
use zksync_web3_decl::{
    client::{Client, DynClient, L1},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

use crate::sdk::{
    error::ClientError,
    ethabi::Bytes,
    operations::SyncTransactionHandle,
    utils::{is_token_eth, load_contract},
};

const IERC20_INTERFACE: &str = include_str!("../abi/IERC20.json");
const HYPERCHAIN_INTERFACE: &str = include_str!("../abi/IZkSyncHyperchain.json");
const L1_ERC20_BRIDGE_INTERFACE: &str = include_str!("../abi/IL1ERC20Bridge.json");
const RAW_ERC20_DEPOSIT_GAS_LIMIT: &str = include_str!("DepositERC20GasLimit.json");

// The `gasPerPubdata` to be used in L1->L2 requests. It may be almost any number, but here we 800
// as an optimal one. In the future, it will be estimated.
const L1_TO_L2_GAS_PER_PUBDATA: u32 = 800;

/// Returns `ethabi::Contract` object for an interface of a hyperchain
pub fn hyperchain_contract() -> ethabi::Contract {
    load_contract(HYPERCHAIN_INTERFACE)
}

/// Returns `ethabi::Contract` object for ERC-20 smart contract interface.
pub fn ierc20_contract() -> ethabi::Contract {
    load_contract(IERC20_INTERFACE)
}

pub fn l1_erc20_bridge_contract() -> ethabi::Contract {
    load_contract(L1_ERC20_BRIDGE_INTERFACE)
}

/// `EthereumProvider` gains access to on-chain operations, such as deposits and full exits.
/// Methods to interact with Ethereum return corresponding Ethereum transaction hash.
/// In order to monitor transaction execution, an Ethereum node `web3` API is exposed
/// via `EthereumProvider::web3` method.
#[derive(Debug)]
pub struct EthereumProvider<S: EthereumSigner> {
    eth_client: SigningClient<S, L1>,
    default_bridges: BridgeAddresses,
    erc20_abi: ethabi::Contract,
    l1_erc20_bridge_abi: ethabi::Contract,
    confirmation_timeout: Duration,
    polling_interval: Duration,
}

// TODO (SMA-1623): create a way to pass `Options` (e.g. `nonce`, `gas_limit`, `priority_fee_per_gas`)
// into methods that perform L1 transactions. The unit is wei.
pub const DEFAULT_PRIORITY_FEE: u64 = 2_000_000_000;

impl<S: EthereumSigner> EthereumProvider<S> {
    /// Creates a new Ethereum provider.
    pub async fn new<P>(
        provider: &P,
        eth_web3_url: impl AsRef<str>,
        eth_signer: S,
        eth_addr: H160,
    ) -> Result<Self, ClientError>
    where
        P: ZksNamespaceClient + Sync,
    {
        let l1_chain_id = provider.l1_chain_id().await?;
        let l1_chain_id = u64::try_from(l1_chain_id).map_err(|_| {
            ClientError::MalformedResponse(
                "Chain id overflow - Expected chain id to be in range 0..2^64".to_owned(),
            )
        })?;
        let sl_chain_id = SLChainId(l1_chain_id);

        let contract_address = provider.get_main_l1_contract().await?;
        let default_bridges = provider
            .get_bridge_contracts()
            .await
            .map_err(|err| ClientError::NetworkError(err.to_string()))?;

        let eth_web3_url = eth_web3_url
            .as_ref()
            .parse::<SensitiveUrl>()
            .map_err(|err| ClientError::NetworkError(err.to_string()))?;
        let query_client = Client::http(eth_web3_url)
            .map_err(|err| ClientError::NetworkError(err.to_string()))?
            .for_network(sl_chain_id.into())
            .report_config(false)
            .build();
        let query_client: Box<DynClient<L1>> = Box::new(query_client);
        let eth_client = SigningClient::new(
            query_client.for_component("provider"),
            hyperchain_contract(),
            eth_addr,
            eth_signer,
            contract_address,
            DEFAULT_PRIORITY_FEE.into(),
            sl_chain_id,
        );
        let erc20_abi = ierc20_contract();
        let l1_erc20_bridge_abi = l1_erc20_bridge_contract();

        Ok(Self {
            eth_client,
            default_bridges,
            erc20_abi,
            l1_erc20_bridge_abi,
            confirmation_timeout: Duration::from_secs(10),
            polling_interval: Duration::from_secs(1),
        })
    }

    /// Exposes Ethereum node `web3` API.
    pub fn client(&self) -> &dyn BoundEthInterface {
        &self.eth_client
    }

    pub fn query_client(&self) -> &dyn EthInterface {
        self.eth_client.as_ref()
    }

    /// Returns the ZKsync contract address.
    pub fn contract_address(&self) -> H160 {
        self.client().contract_addr()
    }

    /// Returns the Ethereum account balance.
    pub async fn balance(&self) -> Result<U256, ClientError> {
        self.client()
            .sender_eth_balance()
            .await
            .map_err(|err| ClientError::NetworkError(err.to_string()))
    }

    /// Returns the ERC20 token account balance.
    pub async fn erc20_balance(
        &self,
        address: Address,
        token_address: Address,
    ) -> Result<U256, ClientError> {
        CallFunctionArgs::new("balanceOf", address)
            .for_contract(token_address, &self.erc20_abi)
            .call(self.query_client())
            .await
            .map_err(|err| match err {
                ContractCallError::EthereumGateway(err) => {
                    ClientError::NetworkError(err.to_string())
                }
                _ => ClientError::MalformedResponse(err.to_string()),
            })
    }

    /// Returns the pending nonce for the Ethereum account.
    pub async fn nonce(&self) -> Result<U256, ClientError> {
        self.client()
            .pending_nonce()
            .await
            .map_err(|err| ClientError::NetworkError(err.to_string()))
    }

    /// Checks whether ERC20 of a certain token deposit is approved for account.
    pub async fn is_erc20_deposit_approved(
        &self,
        token_address: Address,
        bridge: Option<Address>,
    ) -> Result<bool, ClientError> {
        self.is_limited_erc20_deposit_approved(token_address, U256::from(2).pow(255.into()), bridge)
            .await
    }

    pub async fn l2_token_address(
        &self,
        l1_token_address: Address,
        bridge: Option<Address>,
    ) -> Result<Address, ClientError> {
        // TODO(EVM-571): This should be moved to the shared bridge, which does not have `l2_token_address` on L1. Use L2 contracts instead.
        let bridge = bridge.unwrap_or(self.default_bridges.l1_erc20_default_bridge.unwrap());
        CallFunctionArgs::new("l2TokenAddress", l1_token_address)
            .for_contract(bridge, &self.l1_erc20_bridge_abi)
            .call(self.query_client())
            .await
            .map_err(|err| match err {
                ContractCallError::EthereumGateway(err) => {
                    ClientError::NetworkError(err.to_string())
                }
                _ => ClientError::MalformedResponse(err.to_string()),
            })
    }

    /// Checks whether ERC20 of a certain token deposit with limit is approved for account.
    pub async fn is_limited_erc20_deposit_approved(
        &self,
        token_address: Address,
        erc20_approve_threshold: U256,
        bridge: Option<Address>,
    ) -> Result<bool, ClientError> {
        // TODO(EVM-571): This should be moved to the shared bridge,
        let bridge = bridge.unwrap_or(self.default_bridges.l1_erc20_default_bridge.unwrap());
        let current_allowance = self
            .client()
            .allowance_on_account(token_address, bridge, &self.erc20_abi)
            .await
            .map_err(|err| ClientError::NetworkError(err.to_string()))?;

        Ok(current_allowance >= erc20_approve_threshold)
    }

    /// Sends a transaction to ERC20 token contract to approve the ERC20 deposit.
    pub async fn approve_erc20_token_deposits(
        &self,
        token_address: Address,
        bridge: Option<Address>,
    ) -> Result<H256, ClientError> {
        self.limited_approve_erc20_token_deposits(token_address, U256::max_value(), bridge)
            .await
    }

    /// Sends a transaction to ERC20 token contract to approve the limited ERC20 deposit.
    pub async fn limited_approve_erc20_token_deposits(
        &self,
        token_address: Address,
        max_erc20_approve_amount: U256,
        bridge: Option<Address>,
    ) -> Result<H256, ClientError> {
        // TODO(EVM-571): This should be moved to the shared bridge,
        let bridge = bridge.unwrap_or(self.default_bridges.l1_erc20_default_bridge.unwrap());
        let contract_function = self
            .erc20_abi
            .function("approve")
            .expect("failed to get function parameters");
        let params = (bridge, max_erc20_approve_amount);
        let data = contract_function
            .encode_input(&params.into_tokens())
            .expect("failed to encode parameters");

        let signed_tx = self
            .client()
            .sign_prepared_tx_for_addr(
                data,
                token_address,
                Options {
                    gas: Some(300_000.into()),
                    ..Default::default()
                },
            )
            .await
            .map_err(|_| ClientError::IncorrectCredentials)?;

        let transaction_hash = self
            .query_client()
            .send_raw_tx(signed_tx.raw_tx)
            .await
            .map_err(|err| ClientError::NetworkError(err.to_string()))?;

        Ok(transaction_hash)
    }

    /// Performs a transfer of funds from one Ethereum account to another.
    /// Note: This operation is performed on Ethereum, and not related to ZKsync directly.
    pub async fn transfer(
        &self,
        token_address: Address,
        amount: U256,
        to: H160,
        options: Option<Options>,
    ) -> Result<H256, ClientError> {
        let signed_tx = if is_token_eth(token_address) {
            let options = Options {
                value: Some(amount),
                gas: Some(300_000.into()),
                ..options.unwrap_or_default()
            };
            self.client()
                .sign_prepared_tx_for_addr(Vec::new(), to, options)
                .await
                .map_err(|_| ClientError::IncorrectCredentials)?
        } else {
            let contract_function = self
                .erc20_abi
                .function("transfer")
                .expect("failed to get function parameters");
            let params = (to, amount);
            let data = contract_function
                .encode_input(&params.into_tokens())
                .expect("failed to encode parameters");

            self.client()
                .sign_prepared_tx_for_addr(
                    data,
                    token_address,
                    Options {
                        gas: Some(300_000.into()),
                        ..options.unwrap_or_default()
                    },
                )
                .await
                .map_err(|_| ClientError::IncorrectCredentials)?
        };

        let transaction_hash = self
            .query_client()
            .send_raw_tx(signed_tx.raw_tx)
            .await
            .map_err(|err| ClientError::NetworkError(err.to_string()))?;

        Ok(transaction_hash)
    }

    pub async fn mint_erc20(
        &self,
        token_address: Address,
        amount: U256,
        to: H160,
    ) -> Result<H256, ClientError> {
        let signed_tx = {
            let contract_function = self
                .erc20_abi
                .function("mint")
                .expect("failed to get function parameters");
            let params = (to, amount);
            let data = contract_function
                .encode_input(&params.into_tokens())
                .expect("failed to encode parameters");

            self.eth_client
                .sign_prepared_tx_for_addr(
                    data,
                    token_address,
                    Options {
                        gas: Some(100_000.into()),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|_| ClientError::IncorrectCredentials)?
        };

        let transaction_hash = self
            .query_client()
            .send_raw_tx(signed_tx.raw_tx)
            .await
            .map_err(|err| ClientError::NetworkError(err.to_string()))?;

        Ok(transaction_hash)
    }

    pub async fn base_cost(
        &self,
        gas_limit: U256,
        gas_per_pubdata_byte: u32,
        gas_price: Option<U256>,
    ) -> Result<U256, ContractCallError> {
        let gas_price = if let Some(gas_price) = gas_price {
            gas_price
        } else {
            self.query_client().get_gas_price().await?
        };
        let args = CallFunctionArgs::new(
            "l2TransactionBaseCost",
            (gas_price, gas_limit, U256::from(gas_per_pubdata_byte)),
        );
        args.for_contract(self.eth_client.contract_addr(), self.eth_client.contract())
            .call(self.query_client())
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn request_execute(
        &self,
        contract_address: Address,
        l2_value: U256,
        calldata: Bytes,
        gas_limit: U256,
        factory_deps: Option<Vec<Bytes>>,
        operator_tip: Option<U256>,
        gas_price: Option<U256>,
        refund_recipient: Address,
    ) -> Result<H256, ClientError> {
        let operator_tip = operator_tip.unwrap_or_default();
        let factory_deps = factory_deps.unwrap_or_default();
        let gas_price = if let Some(gas_price) = gas_price {
            gas_price
        } else {
            self.query_client()
                .get_gas_price()
                .await
                .map_err(|e| ClientError::NetworkError(e.to_string()))?
        };
        let base_cost = self
            .base_cost(gas_limit, L1_TO_L2_GAS_PER_PUBDATA, Some(gas_price))
            .await
            .map_err(|e| ClientError::NetworkError(e.to_string()))?;
        let value = base_cost + operator_tip + l2_value;
        let tx_data = self.client().encode_tx_data(
            "requestL2Transaction",
            (
                contract_address,
                l2_value,
                calldata,
                gas_limit,
                U256::from(L1_TO_L2_GAS_PER_PUBDATA),
                factory_deps,
                refund_recipient,
            )
                .into_tokens(),
        );

        let tx = self
            .client()
            .sign_prepared_tx(
                tx_data,
                Options::with(|f| {
                    f.gas = Some(U256::from(600000));
                    f.value = Some(value);
                    f.gas_price = Some(gas_price)
                }),
            )
            .await
            .map_err(|e| ClientError::NetworkError(e.to_string()))?;

        let tx_hash = self
            .query_client()
            .send_raw_tx(tx.raw_tx)
            .await
            .map_err(|e| ClientError::NetworkError(e.to_string()))?;

        Ok(tx_hash)
    }

    /// Performs a deposit in ZKsync network.
    /// For ERC20 tokens, a deposit must be approved beforehand via the `EthereumProvider::approve_erc20_token_deposits` method.
    #[allow(clippy::too_many_arguments)]
    pub async fn deposit(
        &self,
        l1_token_address: Address,
        amount: U256,
        to: Address,
        operator_tip: Option<U256>,
        bridge_address: Option<Address>,
        eth_options: Option<Options>,
    ) -> Result<H256, ClientError> {
        let operator_tip = operator_tip.unwrap_or_default();

        let is_eth_deposit = l1_token_address == Address::zero();

        // Calculate the gas limit for transaction: it may vary for different tokens.
        let gas_limit = if is_eth_deposit {
            400_000u64
        } else {
            let gas_limits: Map<String, Value> = serde_json::from_str(RAW_ERC20_DEPOSIT_GAS_LIMIT)
                .map_err(|_| ClientError::Other)?;
            let address_str = format!("{:?}", l1_token_address);
            let is_mainnet = Network::from_chain_id(self.client().chain_id()) == Network::Mainnet;
            if is_mainnet && gas_limits.contains_key(&address_str) {
                2 * gas_limits
                    .get(&address_str)
                    .unwrap()
                    .as_u64()
                    .ok_or(ClientError::Other)?
            } else {
                800000u64
            }
        };

        let mut options = eth_options.unwrap_or_default();

        // If the user has already provided `max_fee_per_gas` or `gas_price`, we will use
        // it to calculate the base cost for the transaction
        let gas_price = if let Some(max_fee_per_gas) = options.max_fee_per_gas {
            max_fee_per_gas
        } else if let Some(gas_price) = options.gas_price {
            gas_price
        } else {
            let gas_price = self
                .query_client()
                .get_gas_price()
                .await
                .map_err(|e| ClientError::NetworkError(e.to_string()))?;

            options.gas_price = Some(gas_price);

            gas_price
        };

        // TODO (PLA-85): Add gas estimations for deposits in Rust SDK
        let l2_gas_limit = U256::from(6_000_000u32);

        let base_cost: U256 = self
            .base_cost(
                l2_gas_limit,
                REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE as u32,
                Some(gas_price),
            )
            .await
            .map_err(|e| ClientError::NetworkError(e.to_string()))?;

        // Calculate the amount of ether to be sent in the transaction.
        let total_value = if is_eth_deposit {
            // Both fee component and the deposit amount are represented as `msg.value`.
            base_cost + operator_tip + amount
        } else {
            // ERC20 token, `msg.value` is used only for the fee.
            base_cost + operator_tip
        };

        options.value = Some(total_value);
        options.gas = Some(gas_limit.into());

        let transaction_hash = if is_eth_deposit {
            self.request_execute(
                to,
                amount,
                Default::default(),
                l2_gas_limit,
                None,
                None,
                Some(gas_price),
                Default::default(),
            )
            .await?
        } else {
            // TODO(EVM-571): This should be moved to the shared bridge, and the `requestL2Transaction` method
            let bridge_address =
                bridge_address.unwrap_or(self.default_bridges.l1_erc20_default_bridge.unwrap());
            let contract_function = self
                .l1_erc20_bridge_abi
                .function("deposit")
                .expect("failed to get function parameters");
            let params = (
                to,
                l1_token_address,
                amount,
                l2_gas_limit,
                U256::from(L1_TO_L2_GAS_PER_PUBDATA),
            );
            let data = contract_function
                .encode_input(&params.into_tokens())
                .expect("failed to encode parameters");

            let signed_tx = self
                .eth_client
                .sign_prepared_tx_for_addr(data, bridge_address, options)
                .await
                .map_err(|_| ClientError::IncorrectCredentials)?;
            self.query_client()
                .send_raw_tx(signed_tx.raw_tx)
                .await
                .map_err(|err| ClientError::NetworkError(err.to_string()))?
        };

        Ok(transaction_hash)
    }

    /// Sets the timeout to wait for transactions to appear in the Ethereum network.
    /// By default it is set to 10 seconds.
    pub fn set_confirmation_timeout(&mut self, timeout: Duration) {
        self.confirmation_timeout = timeout;
    }

    pub fn set_polling_interval(&mut self, polling_interval: Duration) {
        self.polling_interval = polling_interval;
    }

    /// Waits until the transaction is confirmed by the Ethereum blockchain.
    pub async fn wait_for_tx(&self, tx_hash: H256) -> Result<TransactionReceipt, ClientError> {
        let mut poller = tokio::time::interval(self.polling_interval);

        let start = Instant::now();
        loop {
            if let Some(receipt) = self
                .query_client()
                .tx_receipt(tx_hash)
                .await
                .map_err(|err| ClientError::NetworkError(err.to_string()))?
            {
                return Ok(receipt);
            }

            if start.elapsed() > self.confirmation_timeout {
                return Err(ClientError::OperationTimeout);
            }
            poller.tick().await;
        }
    }
}

/// Trait describes the ability to receive the priority operation from this holder.
pub trait PriorityOpHolder {
    /// Returns the priority operation if exists.
    fn priority_op(&self) -> Option<L1TxCommonData>;

    /// Returns the handle for the priority operation.
    fn priority_op_handle<'a, P>(&self, provider: &'a P) -> Option<SyncTransactionHandle<'a, P>>
    where
        P: EthNamespaceClient + Sync,
    {
        self.priority_op()
            .map(|op| SyncTransactionHandle::new(op.hash(), provider))
    }
}

impl PriorityOpHolder for TransactionReceipt {
    fn priority_op(&self) -> Option<L1TxCommonData> {
        self.logs
            .iter()
            .find_map(|op| L1Tx::try_from(op.clone()).ok().map(|tx| tx.common_data))
    }
}
