// Built-in uses
use std::collections::HashMap;

// External uses
use bigdecimal::BigDecimal;
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;

// Workspace uses
use zksync_types::{
    api::{BridgeAddresses, L2ToL1LogProof, TransactionDetails},
    explorer_api::BlockDetails,
    fee::Fee,
    transaction_request::CallRequest,
    vm_trace::{ContractSourceDebugInfo, VmDebugTrace},
    Address, Bytes, L1BatchNumber, MiniblockNumber, H256, U256, U64,
};
use zksync_web3_decl::error::Web3Error;
use zksync_web3_decl::types::Token;

// Local uses
use crate::web3::backend_jsonrpc::error::into_jsrpc_error;
use crate::web3::namespaces::ZksNamespace;

#[rpc]
pub trait ZksNamespaceT {
    #[rpc(name = "zks_estimateFee", returns = "Fee")]
    fn estimate_fee(&self, req: CallRequest) -> Result<Fee>;

    #[rpc(name = "zks_getMainContract", returns = "Address")]
    fn get_main_contract(&self) -> Result<Address>;

    #[rpc(name = "zks_getTestnetPaymaster", returns = "Option<Address>")]
    fn get_testnet_paymaster(&self) -> Result<Option<Address>>;

    #[rpc(name = "zks_getBridgeContracts", returns = "BridgeAddresses")]
    fn get_bridge_contracts(&self) -> Result<BridgeAddresses>;

    #[rpc(name = "zks_L1ChainId", returns = "U64")]
    fn l1_chain_id(&self) -> Result<U64>;

    #[rpc(name = "zks_getConfirmedTokens", returns = "Vec<Token>")]
    fn get_confirmed_tokens(&self, from: u32, limit: u8) -> Result<Vec<Token>>;

    #[rpc(name = "zks_getTokenPrice", returns = "BigDecimal")]
    fn get_token_price(&self, token_address: Address) -> Result<BigDecimal>;

    #[rpc(name = "zks_setContractDebugInfo", returns = "bool")]
    fn set_contract_debug_info(
        &self,
        contract_address: Address,
        info: ContractSourceDebugInfo,
    ) -> Result<bool>;

    #[rpc(name = "zks_getContractDebugInfo", returns = "ContractSourceDebugInfo")]
    fn get_contract_debug_info(
        &self,
        contract_address: Address,
    ) -> Result<Option<ContractSourceDebugInfo>>;

    #[rpc(name = "zks_getTransactionTrace", returns = "Option<VmDebugTrace>")]
    fn get_transaction_trace(&self, hash: H256) -> Result<Option<VmDebugTrace>>;

    #[rpc(name = "zks_getAllAccountBalances", returns = "HashMap<Address, U256>")]
    fn get_all_account_balances(&self, address: Address) -> Result<HashMap<Address, U256>>;

    #[rpc(name = "zks_getL2ToL1MsgProof", returns = "Option<Vec<H256>>")]
    fn get_l2_to_l1_msg_proof(
        &self,
        block: MiniblockNumber,
        sender: Address,
        msg: H256,
        l2_log_position: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>>;

    #[rpc(name = "zks_getL2ToL1LogProof", returns = "Option<Vec<H256>>")]
    fn get_l2_to_l1_log_proof(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>>;

    #[rpc(name = "zks_L1BatchNumber", returns = "U64")]
    fn get_l1_batch_number(&self) -> Result<U64>;

    #[rpc(name = "zks_getBlockDetails", returns = "Option<BlockDetails>")]
    fn get_block_details(&self, block_number: MiniblockNumber) -> Result<Option<BlockDetails>>;

    #[rpc(name = "zks_getL1BatchBlockRange", returns = "Option<(U64, U64)>")]
    fn get_miniblock_range(&self, batch: L1BatchNumber) -> Result<Option<(U64, U64)>>;

    #[rpc(name = "zks_setKnownBytecode", returns = "bool")]
    fn set_known_bytecode(&self, bytecode: Bytes) -> Result<bool>;

    #[rpc(
        name = "zks_getTransactionDetails",
        returns = "Option<TransactionDetails>"
    )]
    fn get_transaction_details(&self, hash: H256) -> Result<Option<TransactionDetails>>;
}

impl ZksNamespaceT for ZksNamespace {
    fn estimate_fee(&self, req: CallRequest) -> Result<Fee> {
        self.estimate_fee_impl(req).map_err(into_jsrpc_error)
    }

    fn get_main_contract(&self) -> Result<Address> {
        Ok(self.get_main_contract_impl())
    }

    fn get_miniblock_range(&self, batch: L1BatchNumber) -> Result<Option<(U64, U64)>> {
        self.get_miniblock_range_impl(batch)
            .map_err(into_jsrpc_error)
    }

    fn get_testnet_paymaster(&self) -> Result<Option<Address>> {
        Ok(self.get_testnet_paymaster_impl())
    }

    fn get_bridge_contracts(&self) -> Result<BridgeAddresses> {
        Ok(self.get_bridge_contracts_impl())
    }

    fn l1_chain_id(&self) -> Result<U64> {
        Ok(self.l1_chain_id_impl())
    }

    fn get_confirmed_tokens(&self, from: u32, limit: u8) -> Result<Vec<Token>> {
        self.get_confirmed_tokens_impl(from, limit)
            .map_err(into_jsrpc_error)
    }

    fn get_token_price(&self, token_address: Address) -> Result<BigDecimal> {
        self.get_token_price_impl(token_address)
            .map_err(into_jsrpc_error)
    }

    fn set_contract_debug_info(
        &self,
        address: Address,
        info: ContractSourceDebugInfo,
    ) -> Result<bool> {
        Ok(self.set_contract_debug_info_impl(address, info))
    }

    fn get_contract_debug_info(&self, address: Address) -> Result<Option<ContractSourceDebugInfo>> {
        Ok(self.get_contract_debug_info_impl(address))
    }

    fn get_transaction_trace(&self, hash: H256) -> Result<Option<VmDebugTrace>> {
        Ok(self.get_transaction_trace_impl(hash))
    }

    fn get_all_account_balances(&self, address: Address) -> Result<HashMap<Address, U256>> {
        self.get_all_account_balances_impl(address)
            .map_err(into_jsrpc_error)
    }

    fn get_l2_to_l1_msg_proof(
        &self,
        block: MiniblockNumber,
        sender: Address,
        msg: H256,
        l2_log_position: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>> {
        self.get_l2_to_l1_msg_proof_impl(block, sender, msg, l2_log_position)
            .map_err(into_jsrpc_error)
    }

    fn get_l2_to_l1_log_proof(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>> {
        self.get_l2_to_l1_log_proof_impl(tx_hash, index)
            .map_err(into_jsrpc_error)
    }

    fn get_l1_batch_number(&self) -> Result<U64> {
        self.get_l1_batch_number_impl().map_err(into_jsrpc_error)
    }

    fn get_block_details(&self, block_number: MiniblockNumber) -> Result<Option<BlockDetails>> {
        self.get_block_details_impl(block_number)
            .map_err(into_jsrpc_error)
    }

    fn get_transaction_details(&self, hash: H256) -> Result<Option<TransactionDetails>> {
        self.get_transaction_details_impl(hash)
            .map_err(into_jsrpc_error)
    }

    fn set_known_bytecode(&self, _bytecode: Bytes) -> Result<bool> {
        #[cfg(feature = "openzeppelin_tests")]
        return Ok(self.set_known_bytecode_impl(_bytecode));

        #[cfg(not(feature = "openzeppelin_tests"))]
        Err(into_jsrpc_error(Web3Error::NotImplemented))
    }
}
