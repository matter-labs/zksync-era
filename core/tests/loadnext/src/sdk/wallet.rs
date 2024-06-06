use zksync_eth_signer::EthereumSigner;
use zksync_types::{
    api::{BlockIdVariant, BlockNumber, TransactionRequest},
    l2::L2Tx,
    tokens::ETHEREUM_ADDRESS,
    transaction_request::CallRequest,
    web3::Bytes,
    Address, Eip712Domain, U256,
};
use zksync_web3_decl::{
    client::{Client, L2},
    namespaces::{EthNamespaceClient, NetNamespaceClient, Web3NamespaceClient, ZksNamespaceClient},
};

use crate::sdk::{
    error::ClientError,
    ethereum::{ierc20_contract, EthereumProvider},
    operations::*,
    signer::Signer,
    web3::contract::Tokenizable,
};

#[derive(Debug)]
pub struct Wallet<S: EthereumSigner, P> {
    pub provider: P,
    pub signer: Signer<S>,
}

impl<S> Wallet<S, Client<L2>>
where
    S: EthereumSigner,
{
    /// Creates new wallet with an HTTP client.
    ///
    /// # Panics
    ///
    /// Panics if the provided network is not supported.
    pub fn with_http_client(
        rpc_address: &str,
        signer: Signer<S>,
    ) -> Result<Wallet<S, Client<L2>>, ClientError> {
        let rpc_address = rpc_address
            .parse()
            .map_err(|err| ClientError::NetworkError(format!("error parsing RPC url: {err}")))?;
        let client = Client::http(rpc_address)
            .map_err(|err| ClientError::NetworkError(err.to_string()))?
            .for_network(signer.chain_id.into())
            .build();

        Ok(Wallet {
            provider: client,
            signer,
        })
    }
}

impl<S, P> Wallet<S, P>
where
    S: EthereumSigner,
    P: EthNamespaceClient + ZksNamespaceClient + NetNamespaceClient + Web3NamespaceClient + Sync,
{
    pub fn new(provider: P, signer: Signer<S>) -> Self {
        Self { provider, signer }
    }

    /// Returns the wallet address.
    pub fn address(&self) -> Address {
        self.signer.address
    }

    /// Returns balance in the account.
    pub async fn get_balance(
        &self,
        block_number: BlockNumber,
        token_address: Address,
    ) -> Result<U256, ClientError> {
        let balance = if token_address == ETHEREUM_ADDRESS {
            self.provider
                .get_balance(
                    self.address(),
                    Some(BlockIdVariant::BlockNumber(block_number)),
                )
                .await?
        } else {
            let token_contract = ierc20_contract();
            let contract_function = token_contract
                .function("balanceOf")
                .expect("failed to get `balanceOf` function");
            let data = contract_function
                .encode_input(&[self.address().into_token()])
                .expect("failed to encode parameters");
            let req = CallRequest {
                to: Some(token_address),
                data: Some(data.into()),
                ..Default::default()
            };
            let bytes = self
                .provider
                .call(req, Some(BlockIdVariant::BlockNumber(block_number)))
                .await?;
            if bytes.0.len() == 32 {
                U256::from_big_endian(&bytes.0)
            } else {
                U256::zero()
            }
        };

        Ok(balance)
    }

    /// Returns committed account nonce.
    pub async fn get_nonce(&self) -> Result<u32, ClientError> {
        let nonce = self
            .provider
            .get_transaction_count(
                self.address(),
                Some(BlockIdVariant::BlockNumber(BlockNumber::Committed)),
            )
            .await?
            .as_u32();

        Ok(nonce)
    }

    /// Initializes `Transfer` transaction sending.
    pub fn start_transfer(&self) -> TransferBuilder<'_, S, P> {
        TransferBuilder::new(self)
    }

    /// Initializes `Withdraw` transaction sending.
    pub fn start_withdraw(&self) -> WithdrawBuilder<'_, S, P> {
        WithdrawBuilder::new(self)
    }

    /// Initializes `DeployContract` transaction sending.
    pub fn start_deploy_contract(&self) -> DeployContractBuilder<'_, S, P> {
        DeployContractBuilder::new(self)
    }

    /// Initializes `ExecuteContract` transaction sending.
    pub fn start_execute_contract(&self) -> ExecuteContractBuilder<'_, S, P> {
        ExecuteContractBuilder::new(self)
    }

    /// Submits an L2 transaction.
    pub async fn send_transaction(
        &self,
        tx: L2Tx,
    ) -> Result<SyncTransactionHandle<'_, P>, ClientError> {
        // Since we sign the transaction with the Ethereum signature later on,
        // we might want to get rid of the signature and the initiator left from `L2Tx`.
        let transaction_request: TransactionRequest = {
            let mut req: TransactionRequest = tx.into();
            if let Some(meta) = req.eip712_meta.as_mut() {
                meta.custom_signature = None;
            }
            req.from = Some(self.address());
            req.chain_id = Some(self.signer.chain_id.as_u64());
            req
        };
        let domain = Eip712Domain::new(self.signer.chain_id);
        let signature = self
            .signer
            .eth_signer
            .sign_typed_data(&domain, &transaction_request)
            .await?;

        let encoded_tx = transaction_request
            .get_signed_bytes(&signature)
            .map_err(|_| ClientError::Other)?;
        let bytes = Bytes(encoded_tx);

        let tx_hash = self.provider.send_raw_transaction(bytes).await?;

        Ok(SyncTransactionHandle::new(tx_hash, &self.provider))
    }

    /// Creates an `EthereumProvider` to interact with the Ethereum network.
    ///
    /// Returns an error if wallet was created without providing an Ethereum private key.
    pub async fn ethereum(
        &self,
        web3_addr: impl AsRef<str>,
    ) -> Result<EthereumProvider<S>, ClientError> {
        let ethereum_provider = EthereumProvider::new(
            &self.provider,
            web3_addr,
            self.signer.eth_signer.clone(),
            self.signer.address,
        )
        .await?;

        Ok(ethereum_provider)
    }
}
