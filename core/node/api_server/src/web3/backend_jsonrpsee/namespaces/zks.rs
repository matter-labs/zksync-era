use std::collections::HashMap;

use zksync_multivm::interface::VmEvent;
use zksync_types::{
    api::{
        state_override::StateOverride, ApiStorageLog, BlockDetails, BridgeAddresses,
        L1BatchDetails, L2ToL1LogProof, Log, Proof, ProtocolVersion, TransactionDetailedResult,
        TransactionDetails,
    },
    fee::Fee,
    fee_model::{FeeParams, PubdataIndependentBatchFeeModelInput},
    transaction_request::CallRequest,
    web3, Address, L1BatchNumber, L2BlockNumber, H256, U256, U64,
};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::ZksNamespaceServer,
    types::Token,
};

use crate::web3::ZksNamespace;

#[async_trait]
impl ZksNamespaceServer for ZksNamespace {
    async fn estimate_fee(
        &self,
        req: CallRequest,
        state_override: Option<StateOverride>,
    ) -> RpcResult<Fee> {
        self.estimate_fee_impl(req, state_override)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn estimate_gas_l1_to_l2(
        &self,
        req: CallRequest,
        state_override: Option<StateOverride>,
    ) -> RpcResult<U256> {
        self.estimate_l1_to_l2_gas_impl(req, state_override)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_bridgehub_contract(&self) -> RpcResult<Option<Address>> {
        Ok(self.get_bridgehub_contract_impl())
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
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_all_account_balances(
        &self,
        address: Address,
    ) -> RpcResult<HashMap<Address, U256>> {
        self.get_all_account_balances_impl(address)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_l2_to_l1_msg_proof(
        &self,
        block: L2BlockNumber,
        sender: Address,
        msg: H256,
        l2_log_position: Option<usize>,
    ) -> RpcResult<Option<L2ToL1LogProof>> {
        self.get_l2_to_l1_msg_proof_impl(block, sender, msg, l2_log_position)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_l2_to_l1_log_proof(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> RpcResult<Option<L2ToL1LogProof>> {
        self.get_l2_to_l1_log_proof_impl(tx_hash, index)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_l1_batch_number(&self) -> RpcResult<U64> {
        self.get_l1_batch_number_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_l2_block_range(&self, batch: L1BatchNumber) -> RpcResult<Option<(U64, U64)>> {
        self.get_l2_block_range_impl(batch)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_block_details(
        &self,
        block_number: L2BlockNumber,
    ) -> RpcResult<Option<BlockDetails>> {
        self.get_block_details_impl(block_number)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_transaction_details(&self, hash: H256) -> RpcResult<Option<TransactionDetails>> {
        self.get_transaction_details_impl(hash)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_raw_block_transactions(
        &self,
        block_number: L2BlockNumber,
    ) -> RpcResult<Vec<zksync_types::Transaction>> {
        self.get_raw_block_transactions_impl(block_number)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_l1_batch_details(
        &self,
        batch_number: L1BatchNumber,
    ) -> RpcResult<Option<L1BatchDetails>> {
        self.get_l1_batch_details_impl(batch_number)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_bytecode_by_hash(&self, hash: H256) -> RpcResult<Option<Vec<u8>>> {
        self.get_bytecode_by_hash_impl(hash)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    // to be removed in favor of `get_batch_fee_input`
    async fn get_l1_gas_price(&self) -> RpcResult<U64> {
        match self.get_batch_fee_input_impl().await {
            Ok(fee_input) => Ok(fee_input.l1_gas_price.into()),
            Err(err) => Err(self.current_method().map_err(err)),
        }
    }

    async fn get_fee_params(&self) -> RpcResult<FeeParams> {
        Ok(self.get_fee_params_impl())
    }

    async fn get_batch_fee_input(&self) -> RpcResult<PubdataIndependentBatchFeeModelInput> {
        self.get_batch_fee_input_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_protocol_version(
        &self,
        version_id: Option<u16>,
    ) -> RpcResult<Option<ProtocolVersion>> {
        self.get_protocol_version_impl(version_id)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_proof(
        &self,
        address: Address,
        keys: Vec<H256>,
        l1_batch_number: L1BatchNumber,
    ) -> RpcResult<Option<Proof>> {
        self.get_proofs_impl(address, keys, l1_batch_number)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_base_token_l1_address(&self) -> RpcResult<Address> {
        self.get_base_token_l1_address_impl()
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn send_raw_transaction_with_detailed_output(
        &self,
        tx_bytes: web3::Bytes,
    ) -> RpcResult<TransactionDetailedResult> {
        self.send_raw_transaction_with_detailed_output_impl(tx_bytes)
            .await
            .map(|result| TransactionDetailedResult {
                transaction_hash: result.0,
                storage_logs: result
                    .1
                    .logs
                    .storage_logs
                    .iter()
                    .filter(|x| x.log.is_write())
                    .map(ApiStorageLog::from)
                    .collect(),
                events: result
                    .1
                    .logs
                    .events
                    .iter()
                    .map(|event| {
                        let mut log = map_event(event);
                        log.transaction_hash = Some(result.0);
                        log
                    })
                    .collect(),
            })
            .map_err(|err| self.current_method().map_err(err))
    }
}

fn map_event(vm_event: &VmEvent) -> Log {
    Log {
        address: vm_event.address,
        topics: vm_event.indexed_topics.clone(),
        data: web3::Bytes::from(vm_event.value.clone()),
        block_hash: None,
        block_number: None,
        l1_batch_number: Some(U64::from(vm_event.location.0 .0)),
        transaction_hash: None,
        transaction_index: Some(web3::Index::from(vm_event.location.1)),
        log_index: None,
        transaction_log_index: None,
        log_type: None,
        removed: Some(false),
        block_timestamp: None,
    }
}
