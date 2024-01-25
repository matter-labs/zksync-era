use std::sync::Arc;

use async_trait::async_trait;
use zksync_types::{
    web3::{
        contract::Options,
        ethabi,
        types::{
            Address, Block, BlockId, BlockNumber, Filter, Log, Transaction, TransactionReceipt,
            H160, H256, U256, U64,
        },
    },
    L1ChainId,
};

use crate::{
    BoundEthInterface, ContractCall, Error, EthInterface, ExecutedTxStatus, FailureInfo,
    RawTransactionBytes, SignedCallResult,
};

#[async_trait]
impl<C: EthInterface + ?Sized> EthInterface for Arc<C> {
    async fn nonce_at_for_account(
        &self,
        account: Address,
        block: BlockNumber,
        component: &'static str,
    ) -> Result<U256, Error> {
        self.as_ref()
            .nonce_at_for_account(account, block, component)
            .await
    }

    async fn base_fee_history(
        &self,
        from_block: usize,
        block_count: usize,
        component: &'static str,
    ) -> Result<Vec<u64>, Error> {
        self.as_ref()
            .base_fee_history(from_block, block_count, component)
            .await
    }

    async fn get_pending_block_base_fee_per_gas(
        &self,
        component: &'static str,
    ) -> Result<U256, Error> {
        self.as_ref()
            .get_pending_block_base_fee_per_gas(component)
            .await
    }

    async fn get_gas_price(&self, component: &'static str) -> Result<U256, Error> {
        self.as_ref().get_gas_price(component).await
    }

    async fn block_number(&self, component: &'static str) -> Result<U64, Error> {
        self.as_ref().block_number(component).await
    }

    async fn send_raw_tx(&self, tx: RawTransactionBytes) -> Result<H256, Error> {
        self.as_ref().send_raw_tx(tx).await
    }

    async fn get_tx_status(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<ExecutedTxStatus>, Error> {
        self.as_ref().get_tx_status(hash, component).await
    }

    async fn failure_reason(&self, tx_hash: H256) -> Result<Option<FailureInfo>, Error> {
        self.as_ref().failure_reason(tx_hash).await
    }

    async fn get_tx(
        &self,
        hash: H256,
        component: &'static str,
    ) -> Result<Option<Transaction>, Error> {
        self.as_ref().get_tx(hash, component).await
    }

    async fn tx_receipt(
        &self,
        tx_hash: H256,
        component: &'static str,
    ) -> Result<Option<TransactionReceipt>, Error> {
        self.as_ref().tx_receipt(tx_hash, component).await
    }

    async fn eth_balance(&self, address: Address, component: &'static str) -> Result<U256, Error> {
        self.as_ref().eth_balance(address, component).await
    }

    async fn call_contract_function(
        &self,
        call: ContractCall,
    ) -> Result<Vec<ethabi::Token>, Error> {
        self.as_ref().call_contract_function(call).await
    }

    async fn logs(&self, filter: Filter, component: &'static str) -> Result<Vec<Log>, Error> {
        self.as_ref().logs(filter, component).await
    }

    async fn block(
        &self,
        block_id: BlockId,
        component: &'static str,
    ) -> Result<Option<Block<H256>>, Error> {
        self.as_ref().block(block_id, component).await
    }
}

#[async_trait::async_trait]
impl<C: BoundEthInterface + ?Sized> BoundEthInterface for Arc<C> {
    fn contract(&self) -> &ethabi::Contract {
        self.as_ref().contract()
    }

    fn contract_addr(&self) -> H160 {
        self.as_ref().contract_addr()
    }

    fn chain_id(&self) -> L1ChainId {
        self.as_ref().chain_id()
    }

    fn sender_account(&self) -> Address {
        self.as_ref().sender_account()
    }

    async fn allowance_on_account(
        &self,
        token_address: Address,
        contract_address: Address,
        erc20_abi: ethabi::Contract,
    ) -> Result<U256, Error> {
        self.as_ref()
            .allowance_on_account(token_address, contract_address, erc20_abi)
            .await
    }

    async fn sign_prepared_tx_for_addr(
        &self,
        data: Vec<u8>,
        contract_addr: H160,
        options: Options,
        component: &'static str,
    ) -> Result<SignedCallResult, Error> {
        self.as_ref()
            .sign_prepared_tx_for_addr(data, contract_addr, options, component)
            .await
    }

    async fn nonce_at(&self, block: BlockNumber, component: &'static str) -> Result<U256, Error> {
        self.as_ref().nonce_at(block, component).await
    }

    async fn current_nonce(&self, component: &'static str) -> Result<U256, Error> {
        self.as_ref().current_nonce(component).await
    }

    async fn pending_nonce(&self, component: &'static str) -> Result<U256, Error> {
        self.as_ref().pending_nonce(component).await
    }
}
