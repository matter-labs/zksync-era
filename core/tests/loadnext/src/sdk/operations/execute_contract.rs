use zksync_eth_signer::EthereumSigner;
use zksync_types::{
    fee::Fee, l2::L2Tx, transaction_request::PaymasterParams, Address, Nonce, U256,
};

use crate::sdk::{
    error::ClientError, operations::SyncTransactionHandle, wallet::Wallet, EthNamespaceClient,
    ZksNamespaceClient,
};

pub struct ExecuteContractBuilder<'a, S: EthereumSigner, P> {
    wallet: &'a Wallet<S, P>,
    contract_address: Option<Address>,
    calldata: Option<Vec<u8>>,
    fee: Option<Fee>,
    value: Option<U256>,
    nonce: Option<Nonce>,
    factory_deps: Option<Vec<Vec<u8>>>,
    paymaster_params: Option<PaymasterParams>,
}

impl<'a, S, P> ExecuteContractBuilder<'a, S, P>
where
    S: EthereumSigner,
    P: ZksNamespaceClient + EthNamespaceClient + Sync,
{
    /// Initializes a change public key transaction building process.
    pub fn new(wallet: &'a Wallet<S, P>) -> Self {
        Self {
            wallet,
            contract_address: None,
            calldata: None,
            fee: None,
            nonce: None,
            value: None,
            factory_deps: None,
            paymaster_params: None,
        }
    }

    /// Sends the transaction, returning the handle for its awaiting.
    pub async fn tx(self) -> Result<L2Tx, ClientError> {
        let paymaster_params = self.paymaster_params.clone().unwrap_or_default();

        let fee = match self.fee {
            Some(fee) => fee,
            None => self.estimate_fee(Some(paymaster_params.clone())).await?,
        };

        let contract_address = self
            .contract_address
            .ok_or_else(|| ClientError::MissingRequiredField("contract_address".into()))?;

        let calldata = self
            .calldata
            .ok_or_else(|| ClientError::MissingRequiredField("calldata".into()))?;

        let nonce = match self.nonce {
            Some(nonce) => nonce,
            None => Nonce(self.wallet.get_nonce().await?),
        };

        self.wallet
            .signer
            .sign_execute_contract(
                contract_address,
                calldata,
                fee,
                nonce,
                self.factory_deps.unwrap_or_default(),
                paymaster_params,
            )
            .await
            .map_err(ClientError::SigningError)
    }

    /// Sends the transaction, returning the handle for its awaiting.
    pub async fn send(self) -> Result<SyncTransactionHandle<'a, P>, ClientError> {
        let wallet = self.wallet;
        let tx = self.tx().await?;

        wallet.send_transaction(tx).await
    }

    /// Sets the calldata for the transaction.
    pub fn calldata(mut self, calldata: Vec<u8>) -> Self {
        self.calldata = Some(calldata);
        self
    }

    /// Sets the value for the transaction.
    pub fn value(mut self, value: U256) -> Self {
        self.value = Some(value);
        self
    }

    /// Sets the transaction contract address.
    pub fn contract_address(mut self, address: Address) -> Self {
        self.contract_address = Some(address);
        self
    }

    /// Set the fee amount.
    ///
    /// For more details, see [utils](../utils/index.html) functions.
    pub fn fee(mut self, fee: Fee) -> Self {
        self.fee = Some(fee);
        self
    }

    /// Sets the transaction nonce.
    pub fn nonce(mut self, nonce: Nonce) -> Self {
        self.nonce = Some(nonce);
        self
    }

    /// Set factory deps
    pub fn factory_deps(mut self, factory_deps: Vec<Vec<u8>>) -> Self {
        self.factory_deps = Some(factory_deps);
        self
    }

    /// Sets the paymaster parameters.
    pub fn paymaster_params(mut self, paymaster_params: PaymasterParams) -> Self {
        self.paymaster_params = Some(paymaster_params);
        self
    }

    pub async fn estimate_fee(
        &self,
        paymaster_params: Option<PaymasterParams>,
    ) -> Result<Fee, ClientError> {
        let contract_address = self
            .contract_address
            .ok_or_else(|| ClientError::MissingRequiredField("contract_address".into()))?;

        let calldata = self
            .calldata
            .clone()
            .ok_or_else(|| ClientError::MissingRequiredField("calldata".into()))?;

        let paymaster_params = paymaster_params
            .or_else(|| self.paymaster_params.clone())
            .unwrap_or_default();

        let execute = L2Tx::new(
            contract_address,
            calldata,
            Nonce(0),
            Default::default(),
            self.wallet.address(),
            self.value.unwrap_or_default(),
            self.factory_deps.clone().unwrap_or_default(),
            paymaster_params,
        );
        self.wallet
            .provider
            .estimate_fee(execute.into())
            .await
            .map_err(Into::into)
    }
}
