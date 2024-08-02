use zksync_eth_signer::EthereumSigner;
use zksync_types::{
    ethabi, fee::Fee, l2::L2Tx, tokens::ETHEREUM_ADDRESS, transaction_request::PaymasterParams,
    Address, Nonce, L2_BASE_TOKEN_ADDRESS, U256,
};

use crate::sdk::{
    error::ClientError,
    operations::{ExecuteContractBuilder, SyncTransactionHandle},
    wallet::Wallet,
    L2EthNamespaceClient, ZksNamespaceClient,
};

pub struct WithdrawBuilder<'a, S: EthereumSigner, P> {
    wallet: &'a Wallet<S, P>,
    to: Option<Address>,
    token: Option<Address>,
    amount: Option<U256>,
    fee: Option<Fee>,
    nonce: Option<Nonce>,
    bridge: Option<Address>,
    paymaster_params: Option<PaymasterParams>,
}

impl<'a, S, P> WithdrawBuilder<'a, S, P>
where
    S: EthereumSigner,
    P: ZksNamespaceClient + L2EthNamespaceClient + Sync,
{
    /// Initializes a withdraw transaction building process.
    pub fn new(wallet: &'a Wallet<S, P>) -> Self {
        Self {
            wallet,
            to: None,
            token: None,
            amount: None,
            fee: None,
            nonce: None,
            bridge: None,
            paymaster_params: None,
        }
    }

    async fn get_execute_builder(&self) -> Result<ExecuteContractBuilder<'_, S, P>, ClientError> {
        let token = self
            .token
            .ok_or_else(|| ClientError::MissingRequiredField("token".into()))?;
        let to = self
            .to
            .ok_or_else(|| ClientError::MissingRequiredField("to".into()))?;
        let amount = self
            .amount
            .ok_or_else(|| ClientError::MissingRequiredField("amount".into()))?;

        let (contract_address, calldata, value) = if token == ETHEREUM_ADDRESS {
            // TODO (SMA-1608): Do not implement the ABI manually, introduce ABI files with an update script similarly to
            //  how it's done for L1 part of SDK.
            let calldata_params = vec![ethabi::ParamType::Address];
            let mut calldata = ethabi::short_signature("withdraw", &calldata_params).to_vec();
            calldata.append(&mut ethabi::encode(&[ethabi::Token::Address(to)]));
            (L2_BASE_TOKEN_ADDRESS, calldata, amount)
        } else {
            let bridge_address = if let Some(bridge) = self.bridge {
                bridge
            } else {
                // Use the default bridge if one was not specified.
                let default_bridges = self
                    .wallet
                    .provider
                    .get_bridge_contracts()
                    .await
                    .map_err(|err| ClientError::NetworkError(err.to_string()))?;
                // Note, that this is safe, but only for Era
                default_bridges.l2_erc20_default_bridge.unwrap()
            };

            // TODO (SMA-1608): Do not implement the ABI manually, introduce ABI files with an update script similarly to
            //  how it's done for L1 part of SDK.
            let calldata_params = vec![
                ethabi::ParamType::Address,
                ethabi::ParamType::Address,
                ethabi::ParamType::Uint(256),
            ];
            let mut calldata = ethabi::short_signature("withdraw", &calldata_params).to_vec();
            calldata.append(&mut ethabi::encode(&[
                ethabi::Token::Address(to),
                ethabi::Token::Address(token),
                ethabi::Token::Uint(amount),
            ]));
            (bridge_address, calldata, U256::zero())
        };

        let paymaster_params = self.paymaster_params.clone().unwrap_or_default();

        let mut builder = ExecuteContractBuilder::new(self.wallet)
            .contract_address(contract_address)
            .calldata(calldata)
            .value(value)
            .paymaster_params(paymaster_params);

        if let Some(fee) = self.fee.clone() {
            builder = builder.fee(fee);
        }
        if let Some(nonce) = self.nonce {
            builder = builder.nonce(nonce);
        }

        Ok(builder)
    }

    /// Directly returns the signed withdraw transaction for the subsequent usage.
    pub async fn tx(self) -> Result<L2Tx, ClientError> {
        let builder = self.get_execute_builder().await?;
        builder.tx().await
    }

    /// Sends the transaction, returning the handle for its awaiting.
    pub async fn send(self) -> Result<SyncTransactionHandle<'a, P>, ClientError> {
        let wallet = self.wallet;
        let tx = self.tx().await?;

        wallet.send_transaction(tx).await
    }

    /// Set the withdrawal amount.
    ///
    /// For more details, see [utils](../utils/index.html) functions.
    pub fn amount(mut self, amount: U256) -> Self {
        self.amount = Some(amount);
        self
    }

    /// Set the withdrawal token.
    pub fn token(mut self, token: Address) -> Self {
        self.token = Some(token);
        self
    }

    /// Set the fee amount.
    ///
    /// For more details, see [utils](../utils/index.html) functions.
    pub fn fee(mut self, fee: Fee) -> Self {
        self.fee = Some(fee);

        self
    }

    /// Sets the address of Ethereum wallet to withdraw funds to.
    pub fn to(mut self, to: Address) -> Self {
        self.to = Some(to);
        self
    }

    /// Same as `WithdrawBuilder::to`, but accepts a string address value.
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

    /// Sets the bridge contract to request the withdrawal.
    pub fn bridge(mut self, address: Address) -> Self {
        self.bridge = Some(address);
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
        let builder = self.get_execute_builder().await?;
        builder.estimate_fee(paymaster_params).await
    }
}
