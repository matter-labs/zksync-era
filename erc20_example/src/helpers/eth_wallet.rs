use std::sync::Arc;

use ethers::{
    prelude::{
        k256::{
            ecdsa::{RecoveryId, Signature as RecoverableSignature},
            schnorr::signature::hazmat::PrehashSigner,
        },
        MiddlewareBuilder, SignerMiddleware,
    },
    providers::Middleware,
    signers::{Signer, Wallet},
    types::{Address, Eip1559TransactionRequest, H256},
};
use zksync_types::{l1, U256};
use zksync_web3_rs::{zks_provider::ZKSProvider, zks_wallet::DepositRequest, ZKSWalletError};

#[derive(Clone, Debug)]
pub struct EthWallet<M, D>
where
    M: Middleware + Clone,
    D: PrehashSigner<(RecoverableSignature, RecoveryId)> + Clone,
{
    /// Eth provider
    pub eth_provider: Option<Arc<SignerMiddleware<M, Wallet<D>>>>,
    pub l1_wallet: Wallet<D>,
}

impl<M, D> EthWallet<M, D>
where
    M: Middleware + 'static + Clone,
    D: PrehashSigner<(RecoverableSignature, RecoveryId)> + Sync + Send + Clone,
{
    pub fn new(
        l1_wallet: Wallet<D>,
        eth_provider: Option<M>,
    ) -> Result<Self, ZKSWalletError<M, D>> {
        Ok(Self {
            l1_wallet: l1_wallet.clone(),
            eth_provider: eth_provider.map(|p| p.with_signer(l1_wallet).into()),
        })
    }

    pub fn get_eth_provider(
        &self,
    ) -> Result<Arc<SignerMiddleware<M, Wallet<D>>>, ZKSWalletError<M, D>> {
        match &self.eth_provider {
            Some(eth_provider) => Ok(Arc::clone(eth_provider)),
            None => Err(ZKSWalletError::NoL1ProviderError()),
        }
    }

    pub fn l1_address(&self) -> Address {
        self.l1_wallet.address()
    }

    pub async fn deposit(&self, request: &DepositRequest) -> Result<H256, ZKSWalletError<M, D>>
    where
        M: ZKSProvider,
    {
        let eth_provider = self.get_eth_provider()?;
        let chain_id = eth_provider.get_chainid().await?.as_u64();

        let tx = Eip1559TransactionRequest {
            from: Some(eth_provider.address()),
            to: Some(request.to.unwrap().into()),
            gas: Some(request.gas_limit),
            value: Some(request.amount),
            data: None,
            nonce: None,
            access_list: Default::default(),
            max_priority_fee_per_gas: None, // FIXME
            max_fee_per_gas: None,          // FIXME
            chain_id: Some(chain_id.into()),
        };
        let pending_transaction = eth_provider.send_transaction(tx, None).await?;

        let receipt = pending_transaction
            .await?
            .ok_or(ZKSWalletError::CustomError(
                "no transaction receipt".to_owned(),
            ));

        Ok(receipt?.transaction_hash)
    }
}
