use zksync_eth_signer::EthereumSigner;
use zksync_types::{
    l2::L2Tx, transaction_request::PaymasterParams, Execute, Nonce, CONTRACT_DEPLOYER_ADDRESS, U256,
};
use zksync_utils::bytecode::hash_bytecode;

use crate::sdk::{
    error::ClientError, operations::SyncTransactionHandle, wallet::Wallet, zksync_types::fee::Fee,
    EthNamespaceClient, ZksNamespaceClient,
};

pub struct DeployContractBuilder<'a, S: EthereumSigner, P> {
    wallet: &'a Wallet<S, P>,
    bytecode: Option<Vec<u8>>,
    calldata: Option<Vec<u8>>,
    fee: Option<Fee>,
    nonce: Option<Nonce>,
    value: Option<U256>,
    factory_deps: Option<Vec<Vec<u8>>>,
    paymaster_params: Option<PaymasterParams>,
}

impl<'a, S, P> DeployContractBuilder<'a, S, P>
where
    S: EthereumSigner,
    P: ZksNamespaceClient + EthNamespaceClient + Sync,
{
    /// Initializes a change public key transaction building process.
    pub fn new(wallet: &'a Wallet<S, P>) -> Self {
        Self {
            wallet,
            bytecode: None,
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

        let bytecode = self
            .bytecode
            .ok_or_else(|| ClientError::MissingRequiredField("bytecode".into()))?;

        let calldata = self.calldata.unwrap_or_default();

        let nonce = match self.nonce {
            Some(nonce) => nonce,
            None => Nonce(self.wallet.get_nonce().await?),
        };

        let main_contract_hash = hash_bytecode(&bytecode);
        let execute_calldata =
            Execute::encode_deploy_params_create(Default::default(), main_contract_hash, calldata);

        let mut factory_deps = self.factory_deps.unwrap_or_default();
        factory_deps.push(bytecode.clone());

        self.wallet
            .signer
            .sign_execute_contract_for_deploy(
                CONTRACT_DEPLOYER_ADDRESS,
                execute_calldata,
                fee,
                nonce,
                vec![bytecode.clone()],
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

    /// Sets the calldata for deploying
    pub fn constructor_calldata(mut self, calldata: Vec<u8>) -> Self {
        self.calldata = Some(calldata);
        self
    }

    /// Sets the factory deps for deploying
    pub fn factory_deps(mut self, factory_deps: Vec<Vec<u8>>) -> Self {
        self.factory_deps = Some(factory_deps);
        self
    }

    /// Sets the deploy contract transaction bytecode.
    pub fn bytecode(mut self, bytecode: Vec<u8>) -> Self {
        self.bytecode = Some(bytecode);
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

    /// Sets the paymaster parameters.
    pub fn paymaster_params(mut self, paymaster_params: PaymasterParams) -> Self {
        self.paymaster_params = Some(paymaster_params);
        self
    }

    pub async fn estimate_fee(
        &self,
        paymaster_params: Option<PaymasterParams>,
    ) -> Result<Fee, ClientError> {
        let bytecode = self
            .bytecode
            .clone()
            .ok_or_else(|| ClientError::MissingRequiredField("bytecode".into()))?;

        let paymaster_params = paymaster_params
            .or_else(|| self.paymaster_params.clone())
            .unwrap_or_default();

        let calldata = self.calldata.clone().unwrap_or_default();
        let main_contract_hash = hash_bytecode(&bytecode);
        let mut factory_deps = self.factory_deps.clone().unwrap_or_default();
        factory_deps.push(bytecode);
        let l2_tx = L2Tx::new(
            CONTRACT_DEPLOYER_ADDRESS,
            Execute::encode_deploy_params_create(Default::default(), main_contract_hash, calldata),
            Nonce(0),
            Default::default(),
            self.wallet.address(),
            self.value.unwrap_or_default(),
            factory_deps,
            paymaster_params,
        );
        self.wallet
            .provider
            .estimate_fee(l2_tx.into(), None)
            .await
            .map_err(Into::into)
    }
}
