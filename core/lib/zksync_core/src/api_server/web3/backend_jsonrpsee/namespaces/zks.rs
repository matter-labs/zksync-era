use bigdecimal::BigDecimal;

use std::collections::HashMap;

use zksync_types::{
    api::{
        BlockDetails, BridgeAddresses, L1BatchDetails, L2ToL1LogProof, Proof, ProtocolVersion,
        TransactionDetails,
    },
    fee::Fee,
    transaction_request::CallRequest,
    Address, L1BatchNumber, MiniblockNumber, H256, U256, U64,
};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::zks::ZksNamespaceServer,
    types::{Filter, Log, Token},
};

use crate::{
    api_server::web3::{backend_jsonrpsee::into_jsrpc_error, ZksNamespace},
    l1_gas_price::L1GasPriceProvider,
};

#[async_trait]
impl<G: L1GasPriceProvider + Send + Sync + 'static> ZksNamespaceServer for ZksNamespace<G> {
    async fn estimate_fee(&self, req: CallRequest) -> RpcResult<Fee> {
        self.estimate_fee_impl(req).await.map_err(into_jsrpc_error)
    }

    async fn estimate_gas_l1_to_l2(&self, req: CallRequest) -> RpcResult<U256> {
        self.estimate_l1_to_l2_gas_impl(req)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_main_contract(&self) -> RpcResult<Address> {
        Ok(self.get_main_contract_impl())
    }

    async fn get_testnet_paymaster(&self) -> RpcResult<Option<Address>> {
        Ok(self.get_testnet_paymaster_impl())
    }

    async fn get_bridge_contracts(&self) -> RpcResult<BridgeAddresses> {
        Ok(self.get_bridge_contracts_impl())
    }

    async fn l1_chain_id(&self) -> RpcResult<U64> {
        Ok(self.l1_chain_id_impl())
    }

    async fn get_confirmed_tokens(&self, from: u32, limit: u8) -> RpcResult<Vec<Token>> {
        self.get_confirmed_tokens_impl(from, limit)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_token_price(&self, token_address: Address) -> RpcResult<BigDecimal> {
        self.get_token_price_impl(token_address)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_all_account_balances(
        &self,
        address: Address,
    ) -> RpcResult<HashMap<Address, U256>> {
        self.get_all_account_balances_impl(address)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_l2_to_l1_msg_proof(
        &self,
        block: MiniblockNumber,
        sender: Address,
        msg: H256,
        l2_log_position: Option<usize>,
    ) -> RpcResult<Option<L2ToL1LogProof>> {
        self.get_l2_to_l1_msg_proof_impl(block, sender, msg, l2_log_position)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_l2_to_l1_log_proof(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> RpcResult<Option<L2ToL1LogProof>> {
        self.get_l2_to_l1_log_proof_impl(tx_hash, index)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_l1_batch_number(&self) -> RpcResult<U64> {
        self.get_l1_batch_number_impl()
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_miniblock_range(&self, batch: L1BatchNumber) -> RpcResult<Option<(U64, U64)>> {
        self.get_miniblock_range_impl(batch)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_block_details(
        &self,
        block_number: MiniblockNumber,
    ) -> RpcResult<Option<BlockDetails>> {
        self.get_block_details_impl(block_number)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_transaction_details(&self, hash: H256) -> RpcResult<Option<TransactionDetails>> {
        self.get_transaction_details_impl(hash)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_raw_block_transactions(
        &self,
        block_number: MiniblockNumber,
    ) -> RpcResult<Vec<zksync_types::Transaction>> {
        self.get_raw_block_transactions_impl(block_number)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_l1_batch_details(
        &self,
        batch_number: L1BatchNumber,
    ) -> RpcResult<Option<L1BatchDetails>> {
        self.get_l1_batch_details_impl(batch_number)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_bytecode_by_hash(&self, hash: H256) -> RpcResult<Option<Vec<u8>>> {
        Ok(self.get_bytecode_by_hash_impl(hash).await)
    }

    async fn get_l1_gas_price(&self) -> RpcResult<U64> {
        Ok(self.get_l1_gas_price_impl())
    }

    async fn get_protocol_version(
        &self,
        version_id: Option<u16>,
    ) -> RpcResult<Option<ProtocolVersion>> {
        Ok(self.get_protocol_version_impl(version_id).await)
    }

    async fn get_logs_with_virtual_blocks(&self, filter: Filter) -> RpcResult<Vec<Log>> {
        self.get_logs_with_virtual_blocks_impl(filter)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_proof(
        &self,
        address: Address,
        keys: Vec<H256>,
        l1_batch_number: L1BatchNumber,
    ) -> RpcResult<Proof> {
        self.get_proofs_impl(address, keys, l1_batch_number)
            .await
            .map_err(into_jsrpc_error)
    }
}
