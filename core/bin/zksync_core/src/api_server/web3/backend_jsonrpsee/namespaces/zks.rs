use crate::api_server::web3::namespaces::zks::ZksNamespace;
use bigdecimal::BigDecimal;
use std::collections::HashMap;
use zksync_types::{
    api::{BridgeAddresses, L2ToL1LogProof, TransactionDetails, U64},
    explorer_api::{BlockDetails, L1BatchDetails},
    fee::Fee,
    transaction_request::CallRequest,
    vm_trace::{ContractSourceDebugInfo, VmDebugTrace},
    Address, L1BatchNumber, MiniblockNumber, H256, U256,
};
use zksync_web3_decl::{
    jsonrpsee::{core::RpcResult, types::error::CallError},
    namespaces::zks::ZksNamespaceServer,
    types::Token,
};

impl ZksNamespaceServer for ZksNamespace {
    fn estimate_fee(&self, req: CallRequest) -> RpcResult<Fee> {
        self.estimate_fee_impl(req)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn estimate_gas_l1_to_l2(&self, req: CallRequest) -> RpcResult<U256> {
        self.estimate_l1_to_l2_gas_impl(req)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_main_contract(&self) -> RpcResult<Address> {
        Ok(self.get_main_contract_impl())
    }

    fn get_testnet_paymaster(&self) -> RpcResult<Option<Address>> {
        Ok(self.get_testnet_paymaster_impl())
    }

    fn get_bridge_contracts(&self) -> RpcResult<BridgeAddresses> {
        Ok(self.get_bridge_contracts_impl())
    }

    fn l1_chain_id(&self) -> RpcResult<U64> {
        Ok(self.l1_chain_id_impl())
    }

    fn get_confirmed_tokens(&self, from: u32, limit: u8) -> RpcResult<Vec<Token>> {
        self.get_confirmed_tokens_impl(from, limit)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_token_price(&self, token_address: Address) -> RpcResult<BigDecimal> {
        self.get_token_price_impl(token_address)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn set_contract_debug_info(
        &self,
        address: Address,
        info: ContractSourceDebugInfo,
    ) -> RpcResult<bool> {
        Ok(self.set_contract_debug_info_impl(address, info))
    }

    fn get_contract_debug_info(
        &self,
        address: Address,
    ) -> RpcResult<Option<ContractSourceDebugInfo>> {
        Ok(self.get_contract_debug_info_impl(address))
    }

    fn get_transaction_trace(&self, hash: H256) -> RpcResult<Option<VmDebugTrace>> {
        Ok(self.get_transaction_trace_impl(hash))
    }

    fn get_all_account_balances(&self, address: Address) -> RpcResult<HashMap<Address, U256>> {
        self.get_all_account_balances_impl(address)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_l2_to_l1_msg_proof(
        &self,
        block: MiniblockNumber,
        sender: Address,
        msg: H256,
        l2_log_position: Option<usize>,
    ) -> RpcResult<Option<L2ToL1LogProof>> {
        self.get_l2_to_l1_msg_proof_impl(block, sender, msg, l2_log_position)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_l2_to_l1_log_proof(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> RpcResult<Option<L2ToL1LogProof>> {
        self.get_l2_to_l1_log_proof_impl(tx_hash, index)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_l1_batch_number(&self) -> RpcResult<U64> {
        self.get_l1_batch_number_impl()
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_miniblock_range(&self, batch: L1BatchNumber) -> RpcResult<Option<(U64, U64)>> {
        self.get_miniblock_range_impl(batch)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_block_details(&self, block_number: MiniblockNumber) -> RpcResult<Option<BlockDetails>> {
        self.get_block_details_impl(block_number)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_transaction_details(&self, hash: H256) -> RpcResult<Option<TransactionDetails>> {
        self.get_transaction_details_impl(hash)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_raw_block_transactions(
        &self,
        block_number: MiniblockNumber,
    ) -> RpcResult<Vec<zksync_types::Transaction>> {
        self.get_raw_block_transactions_impl(block_number)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_l1_batch_details(
        &self,
        batch_number: L1BatchNumber,
    ) -> RpcResult<Option<L1BatchDetails>> {
        self.get_l1_batch_details_impl(batch_number)
            .map_err(|err| CallError::from_std_error(err).into())
    }
}
