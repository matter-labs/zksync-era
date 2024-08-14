use std::fmt::Debug;

use zksync_eth_signer::{EthereumSigner, SignerError};
use zksync_types::{
    fee::Fee, l2::L2Tx, transaction_request::PaymasterParams, Address, Eip712Domain, L2ChainId,
    Nonce, PackedEthSignature, L2_BASE_TOKEN_ADDRESS, U256,
};

use crate::sdk::{operations::create_transfer_calldata, types::TransactionRequest};

fn signing_failed_error(err: impl ToString) -> SignerError {
    SignerError::SigningFailed(err.to_string())
}

#[derive(Debug)]
pub struct Signer<S: EthereumSigner> {
    pub(crate) eth_signer: S,
    pub(crate) address: Address,
    pub(crate) chain_id: L2ChainId,
}

impl<S: EthereumSigner> Signer<S> {
    pub fn new(eth_signer: S, address: Address, chain_id: L2ChainId) -> Self {
        Self {
            eth_signer,
            address,
            chain_id,
        }
    }

    pub async fn sign_transaction(
        &self,
        transaction: &L2Tx,
    ) -> Result<PackedEthSignature, SignerError> {
        let domain = Eip712Domain::new(self.chain_id);
        let transaction_request: TransactionRequest = transaction.clone().into();
        self.eth_signer
            .sign_typed_data(&domain, &transaction_request)
            .await
    }

    pub async fn sign_transfer(
        &self,
        to: Address,
        token: Address,
        amount: U256,
        fee: Fee,
        nonce: Nonce,
        paymaster_params: PaymasterParams,
    ) -> Result<L2Tx, SignerError> {
        // Sign Ether transfer
        if token.is_zero() || token == L2_BASE_TOKEN_ADDRESS {
            let mut transfer = L2Tx::new(
                Some(to),
                Default::default(),
                nonce,
                fee,
                self.eth_signer.get_address().await?,
                amount,
                vec![],
                Default::default(),
            );

            let signature = self
                .sign_transaction(&transfer)
                .await
                .map_err(signing_failed_error)?;
            transfer.set_signature(signature);

            return Ok(transfer);
        }

        // Sign ERC-20 transfer
        let data = create_transfer_calldata(to, amount);
        let mut transfer = L2Tx::new(
            Some(token),
            data,
            nonce,
            fee,
            self.eth_signer.get_address().await?,
            U256::zero(),
            vec![],
            paymaster_params,
        );

        let signature = self
            .sign_transaction(&transfer)
            .await
            .map_err(signing_failed_error)?;
        transfer.set_signature(signature);

        Ok(transfer)
    }

    pub async fn sign_execute_contract(
        &self,
        contract: Option<Address>,
        calldata: Vec<u8>,
        fee: Fee,
        nonce: Nonce,
        factory_deps: Vec<Vec<u8>>,
        paymaster_params: PaymasterParams,
    ) -> Result<L2Tx, SignerError> {
        self.sign_execute_contract_for_deploy(
            contract,
            calldata,
            fee,
            nonce,
            factory_deps,
            paymaster_params,
        )
        .await
    }

    pub async fn sign_execute_contract_for_deploy(
        &self,
        contract: Option<Address>,
        calldata: Vec<u8>,
        fee: Fee,
        nonce: Nonce,
        factory_deps: Vec<Vec<u8>>,
        paymaster_params: PaymasterParams,
    ) -> Result<L2Tx, SignerError> {
        let mut execute_contract = L2Tx::new(
            contract,
            calldata,
            nonce,
            fee,
            self.eth_signer.get_address().await?,
            U256::zero(),
            factory_deps,
            paymaster_params,
        );

        let signature = self
            .sign_transaction(&execute_contract)
            .await
            .map_err(signing_failed_error)?;
        execute_contract.set_signature(signature);

        Ok(execute_contract)
    }
}
