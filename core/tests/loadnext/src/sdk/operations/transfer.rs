use zksync_eth_signer::EthereumSigner;
use zksync_types::{fee::Fee, l2::L2Tx, Address, Nonce, L2_BASE_TOKEN_ADDRESS, U256};

use crate::sdk::{
    error::ClientError,
    ethereum::ierc20_contract,
    operations::SyncTransactionHandle,
    wallet::Wallet,
    web3::contract::Tokenize,
    zksync_types::{transaction_request::PaymasterParams, Execute, L2TxCommonData},
    EthNamespaceClient, ZksNamespaceClient,
};

pub struct TransferBuilder<'a, S: EthereumSigner, P> {
    wallet: &'a Wallet<S, P>,
    to: Option<Address>,
    token: Option<Address>,
    amount: Option<U256>,
    fee: Option<Fee>,
    nonce: Option<Nonce>,
    paymaster_params: Option<PaymasterParams>,
}

impl<'a, S, P> TransferBuilder<'a, S, P>
where
    S: EthereumSigner,
    P: ZksNamespaceClient + EthNamespaceClient + Sync,
{
    /// Initializes a transfer transaction building process.
    pub fn new(wallet: &'a Wallet<S, P>) -> Self {
        Self {
            wallet,
            to: None,
            token: None,
            amount: None,
            fee: None,
            nonce: None,
            paymaster_params: None,
        }
    }

    /// Directly returns the signed transfer transaction for the subsequent usage.
    pub async fn tx(self) -> Result<L2Tx, ClientError> {
        let paymaster_params = self.paymaster_params.clone().unwrap_or_default();

        let fee = match self.fee {
            Some(fee) => fee,
            None => self.estimate_fee(Some(paymaster_params.clone())).await?,
        };

        let to = self
            .to
            .ok_or_else(|| ClientError::MissingRequiredField("to".into()))?;
        let token = self
            .token
            .ok_or_else(|| ClientError::MissingRequiredField("token".into()))?;
        let amount = self
            .amount
            .ok_or_else(|| ClientError::MissingRequiredField("amount".into()))?;

        let nonce = match self.nonce {
            Some(nonce) => nonce,
            None => Nonce(self.wallet.get_nonce().await?),
        };

        self.wallet
            .signer
            .sign_transfer(to, token, amount, fee, nonce, paymaster_params)
            .await
            .map_err(ClientError::SigningError)
    }

    /// Sends the transaction, returning the handle for its awaiting.
    pub async fn send(self) -> Result<SyncTransactionHandle<'a, P>, ClientError> {
        let wallet = self.wallet;
        let tx = self.tx().await?;

        wallet.send_transaction(tx).await
    }

    /// Sets the transaction token address.
    pub fn token(mut self, token: Address) -> Self {
        self.token = Some(token);
        self
    }

    /// Set the transfer amount.
    ///
    /// For more details, see [utils](../utils/index.html) functions.
    pub fn amount(mut self, amount: U256) -> Self {
        self.amount = Some(amount);
        self
    }

    /// Set the fee amount.
    ///
    /// For more details, see [utils](../utils/index.html) functions.
    pub fn fee(mut self, fee: Fee) -> Self {
        self.fee = Some(fee);
        self
    }

    /// Sets the transaction recipient.
    pub fn to(mut self, to: Address) -> Self {
        self.to = Some(to);
        self
    }

    /// Same as `TransferBuilder::to`, but accepts a string address value.
    ///
    /// Provided string value must be a correct address in a hexadecimal form,
    /// otherwise an error will be returned.
    pub fn str_to(mut self, to: impl AsRef<str>) -> Result<Self, ClientError> {
        let to: Address = to
            .as_ref()
            .parse()
            .map_err(|_| ClientError::IncorrectAddress)?;

        self.to = Some(to);
        Ok(self)
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
        let to = self
            .to
            .ok_or_else(|| ClientError::MissingRequiredField("to".into()))?;
        let token = self
            .token
            .ok_or_else(|| ClientError::MissingRequiredField("token".into()))?;
        let amount = self
            .amount
            .ok_or_else(|| ClientError::MissingRequiredField("amount".into()))?;

        let paymaster_params = paymaster_params
            .or_else(|| self.paymaster_params.clone())
            .unwrap_or_default();

        let tx = if token.is_zero() || token == L2_BASE_TOKEN_ADDRESS {
            // ETH estimate
            Execute {
                contract_address: Some(to),
                calldata: Default::default(),
                factory_deps: vec![],
                value: amount,
            }
        } else {
            // ERC-20 estimate
            Execute {
                contract_address: Some(token),
                calldata: create_transfer_calldata(to, amount),
                factory_deps: vec![],
                value: Default::default(),
            }
        };
        let common_data = L2TxCommonData {
            initiator_address: self.wallet.address(),
            nonce: Nonce(0),
            paymaster_params,
            ..Default::default()
        };
        let l2_tx = L2Tx {
            common_data,
            execute: tx,
            received_timestamp_ms: 0,
            raw_bytes: None,
        };
        self.wallet
            .provider
            .estimate_fee(l2_tx.into(), None)
            .await
            .map_err(Into::into)
    }
}

pub fn create_transfer_calldata(to: Address, amount: U256) -> Vec<u8> {
    let contract = ierc20_contract();
    let contract_function = contract
        .function("transfer")
        .expect("failed to get function parameters");
    let params = (to, amount);
    contract_function
        .encode_input(&params.into_tokens())
        .expect("failed to encode parameters")
}
