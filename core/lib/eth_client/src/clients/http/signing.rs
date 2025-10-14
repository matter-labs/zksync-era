use std::{fmt, sync::Arc};

use async_trait::async_trait;
use zksync_contracts::hyperchain_contract;
use zksync_eth_signer::{EthereumSigner, PrivateKeySigner, SignerError, TransactionParameters};
use zksync_types::{
    api::TransactionRequest, ethabi, fee::Fee, l2::L2Tx, web3, Address, Eip712Domain,
    K256PrivateKey, Nonce, SLChainId, EIP_4844_TX_TYPE, EIP_712_TX_TYPE, H160, H256, U256,
};
use zksync_web3_decl::client::{DynClient, Network};

use super::{Method, LATENCIES};
use crate::{
    types::{encode_blob_tx_with_sidecar, ContractCallError, SignedCallResult, SigningError},
    BoundEthInterface, CallFunctionArgs, EthInterface, Options, RawTransactionBytes,
};

/// HTTP-based Ethereum client, backed by a private key to sign transactions.
pub type PKSigningClient<Net> = SigningClient<PrivateKeySigner, Net>;

impl<Net: Network> PKSigningClient<Net> {
    pub fn new_raw(
        operator_private_key: K256PrivateKey,
        diamond_proxy_addr: Address,
        default_priority_fee_per_gas: u64,
        chain_id: SLChainId,
        query_client: Box<DynClient<Net>>,
    ) -> Self {
        let operator_address = operator_private_key.address();
        let signer = PrivateKeySigner::new(operator_private_key);
        tracing::info!("Operator address: {operator_address:?}");
        SigningClient::new(
            query_client,
            hyperchain_contract(),
            operator_address,
            signer,
            diamond_proxy_addr,
            default_priority_fee_per_gas.into(),
            chain_id,
        )
    }
}

/// Gas limit value to be used in transaction if for some reason
/// gas limit was not set for it.
///
/// This is an emergency value, which will not be used normally.
const FALLBACK_GAS_LIMIT: u64 = 3_000_000;

const MAX_GAS_PER_PUBDATA_BYTE: u64 = 50_000;

/// HTTP-based client, instantiated for a certain account. This client is capable of signing transactions.
#[derive(Clone)]
pub struct SigningClient<S: EthereumSigner, Net: Network> {
    inner: Arc<EthDirectClientInner<S>>,
    query_client: Box<DynClient<Net>>,
}

struct EthDirectClientInner<S: EthereumSigner> {
    eth_signer: S,
    sender_account: Address,
    contract_addr: H160,
    contract: ethabi::Contract,
    chain_id: SLChainId,
    default_priority_fee_per_gas: U256,
}

impl<S: EthereumSigner, Net: Network> fmt::Debug for SigningClient<S, Net> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We do not want to have a private key in the debug representation.

        f.debug_struct("SigningClient")
            .field("sender_account", &self.inner.sender_account)
            .field("contract_addr", &self.inner.contract_addr)
            .field("chain_id", &self.inner.chain_id)
            .finish()
    }
}

impl<S: EthereumSigner, Net: Network> AsRef<dyn EthInterface> for SigningClient<S, Net> {
    fn as_ref(&self) -> &(dyn EthInterface + 'static) {
        &self.query_client
    }
}

#[async_trait]
impl<S: EthereumSigner, Net: Network> BoundEthInterface for SigningClient<S, Net> {
    fn clone_boxed(&self) -> Box<dyn BoundEthInterface> {
        Box::new(self.clone())
    }

    fn for_component(self: Box<Self>, component_name: &'static str) -> Box<dyn BoundEthInterface> {
        Box::new(Self {
            query_client: self.query_client.for_component(component_name),
            ..*self
        })
    }

    fn contract(&self) -> &ethabi::Contract {
        &self.inner.contract
    }

    fn contract_addr(&self) -> H160 {
        self.inner.contract_addr
    }

    fn chain_id(&self) -> SLChainId {
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
    ) -> Result<SignedCallResult, SigningError> {
        let latency = LATENCIES.direct[&Method::SignPreparedTx].start();
        // Fetch current max priority fee per gas
        let max_priority_fee_per_gas = match options.max_priority_fee_per_gas {
            Some(max_priority_fee_per_gas) => max_priority_fee_per_gas,
            None => self.inner.default_priority_fee_per_gas,
        };

        if options.transaction_type == Some(EIP_4844_TX_TYPE.into()) {
            if options.max_fee_per_blob_gas.is_none() {
                return Err(SigningError::Eip4844MissingMaxFeePerBlobGas);
            }
            if options.blob_versioned_hashes.is_none() {
                return Err(SigningError::Eip4844MissingBlobVersionedHashes);
            }
        }

        // Fetch current base fee and add `max_priority_fee_per_gas`
        let max_fee_per_gas = match options.max_fee_per_gas {
            Some(max_fee_per_gas) => max_fee_per_gas,
            None => {
                self.as_ref().get_pending_block_base_fee_per_gas().await? + max_priority_fee_per_gas
            }
        };

        if max_fee_per_gas < max_priority_fee_per_gas {
            return Err(SigningError::WrongFeeProvided(
                max_fee_per_gas,
                max_priority_fee_per_gas,
            ));
        }

        let nonce = match options.nonce {
            Some(nonce) => nonce,
            None => <dyn BoundEthInterface>::pending_nonce(self).await?,
        };

        let gas = options.gas.unwrap_or_else(|| {
            // Verbosity level is set to `error`, since we expect all the transactions to have
            // a set limit, but don't want to crаsh the application if for some reason in some
            // place limit was not set.
            tracing::error!("No gas limit was set for transaction, using the default limit: {FALLBACK_GAS_LIMIT}");
            U256::from(FALLBACK_GAS_LIMIT)
        });

        let (signed_tx, hash) = if options.transaction_type == Some(EIP_712_TX_TYPE.into()) {
            self.sign_eip712_tx(
                data,
                contract_addr,
                options,
                nonce,
                max_fee_per_gas,
                max_priority_fee_per_gas,
                gas,
            )
            .await?
        } else {
            self.sign_ethereum_compatible_tx(
                data,
                contract_addr,
                options,
                nonce,
                max_fee_per_gas,
                max_priority_fee_per_gas,
                gas,
            )
            .await?
        };
        latency.observe();

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
        erc20_abi: &ethabi::Contract,
    ) -> Result<U256, ContractCallError> {
        let latency = LATENCIES.direct[&Method::Allowance].start();
        let allowance: U256 =
            CallFunctionArgs::new("allowance", (self.inner.sender_account, address))
                .for_contract(token_address, erc20_abi)
                .call(self.as_ref())
                .await?;
        latency.observe();
        Ok(allowance)
    }
}

impl<S: EthereumSigner, Net: Network> SigningClient<S, Net> {
    pub fn new(
        query_client: Box<DynClient<Net>>,
        contract: ethabi::Contract,
        operator_eth_addr: H160,
        eth_signer: S,
        contract_eth_addr: H160,
        default_priority_fee_per_gas: U256,
        chain_id: SLChainId,
    ) -> Self {
        Self {
            inner: Arc::new(EthDirectClientInner {
                sender_account: operator_eth_addr,
                eth_signer,
                contract_addr: contract_eth_addr,
                chain_id,
                contract,
                default_priority_fee_per_gas,
            }),
            query_client,
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn sign_ethereum_compatible_tx(
        &self,
        data: Vec<u8>,
        contract_addr: H160,
        options: Options,
        nonce: U256,
        max_fee_per_gas: U256,
        max_priority_fee_per_gas: U256,
        gas: U256,
    ) -> Result<(Vec<u8>, H256), SigningError> {
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
        if let Some(sidecar) = options.blob_tx_sidecar {
            signed_tx = encode_blob_tx_with_sidecar(
                &signed_tx,
                sidecar,
                options.support_eip7594.unwrap_or_default(),
            );
        }
        let hash = web3::keccak256(&signed_tx).into();
        Ok((signed_tx, hash))
    }

    #[allow(clippy::too_many_arguments)]
    async fn sign_eip712_tx(
        &self,
        data: Vec<u8>,
        contract_addr: H160,
        options: Options,
        nonce: U256,
        max_fee_per_gas: U256,
        max_priority_fee_per_gas: U256,
        gas: U256,
    ) -> Result<(Vec<u8>, H256), SigningError> {
        let domain = Eip712Domain::new(
            self.inner
                .chain_id
                .0
                .try_into()
                .map_err(|_| SigningError::WrongL2Chain)?,
        );
        let max_gas_per_pubdata = options.max_gas_per_pubdata.unwrap_or_else(|| {
            // Verbosity level is set to `error`, since we expect all the transactions to have
            // a set limit, but don't want to crаsh the application if for some reason in some
            // place limit was not set.
            tracing::error!("No max_gas_per_pubdata set for transaction, using the default limit: {MAX_GAS_PER_PUBDATA_BYTE}");
            U256::from(MAX_GAS_PER_PUBDATA_BYTE)
        });

        let tx = L2Tx::new(
            Some(contract_addr),
            data,
            Nonce(nonce.as_u32()),
            Fee {
                gas_limit: gas,
                max_fee_per_gas,
                max_priority_fee_per_gas,
                gas_per_pubdata_limit: max_gas_per_pubdata,
            },
            self.inner.eth_signer.get_address().await?,
            options.value.unwrap_or_default(),
            options.factory_deps.unwrap_or_default(),
            options.paymaster_params.unwrap_or_default(),
        );

        let mut tx_request: TransactionRequest = tx.into();
        tx_request.chain_id = Some(self.inner.chain_id.0);
        let signature = self
            .inner
            .eth_signer
            .sign_typed_data(&domain, &tx_request)
            .await?;
        tx_request.set_signature(&signature);

        let signed_bytes = tx_request
            .get_signed_bytes(&signature)
            .map_err(|err| SigningError::Signer(SignerError::SigningFailed(err.to_string())))?;
        let tx_hash = tx_request
            .get_tx_hash()
            .map_err(|err| SigningError::Signer(SignerError::SigningFailed(err.to_string())))?;

        Ok((signed_bytes, tx_hash))
    }
}
