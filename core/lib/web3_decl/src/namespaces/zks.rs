use crate::types::Token;
use bigdecimal::BigDecimal;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use std::collections::HashMap;
use zksync_types::api::{BridgeAddresses, L2ToL1LogProof, TransactionDetails};
use zksync_types::transaction_request::CallRequest;
use zksync_types::{
    api::U64,
    explorer_api::BlockDetails,
    fee::Fee,
    vm_trace::{ContractSourceDebugInfo, VmDebugTrace},
    Address, H256, U256,
};
use zksync_types::{L1BatchNumber, MiniblockNumber};

#[cfg_attr(
    all(feature = "client", feature = "server"),
    rpc(server, client, namespace = "zks")
)]
#[cfg_attr(
    all(feature = "client", not(feature = "server")),
    rpc(client, namespace = "zks")
)]
#[cfg_attr(
    all(not(feature = "client"), feature = "server"),
    rpc(server, namespace = "zks")
)]
pub trait ZksNamespace {
    #[method(name = "estimateFee")]
    fn estimate_fee(&self, req: CallRequest) -> RpcResult<Fee>;

    #[method(name = "getMainContract")]
    fn get_main_contract(&self) -> RpcResult<Address>;

    #[method(name = "getTestnetPaymaster")]
    fn get_testnet_paymaster(&self) -> RpcResult<Option<Address>>;

    #[method(name = "getBridgeContracts")]
    fn get_bridge_contracts(&self) -> RpcResult<BridgeAddresses>;

    #[method(name = "L1ChainId")]
    fn l1_chain_id(&self) -> RpcResult<U64>;

    #[method(name = "getConfirmedTokens")]
    fn get_confirmed_tokens(&self, from: u32, limit: u8) -> RpcResult<Vec<Token>>;
    #[method(name = "getTokenPrice")]
    fn get_token_price(&self, token_address: Address) -> RpcResult<BigDecimal>;

    #[method(name = "setContractDebugInfo")]
    fn set_contract_debug_info(
        &self,
        address: Address,
        info: ContractSourceDebugInfo,
    ) -> RpcResult<bool>;

    #[method(name = "getContractDebugInfo")]
    fn get_contract_debug_info(
        &self,
        address: Address,
    ) -> RpcResult<Option<ContractSourceDebugInfo>>;

    #[method(name = "getTransactionTrace")]
    fn get_transaction_trace(&self, hash: H256) -> RpcResult<Option<VmDebugTrace>>;

    #[method(name = "getAllAccountBalances")]
    fn get_all_account_balances(&self, address: Address) -> RpcResult<HashMap<Address, U256>>;

    #[method(name = "getL2ToL1MsgProof")]
    fn get_l2_to_l1_msg_proof(
        &self,
        block: MiniblockNumber,
        sender: Address,
        msg: H256,
        l2_log_position: Option<usize>,
    ) -> RpcResult<Option<L2ToL1LogProof>>;

    #[method(name = "getL2ToL1LogProof")]
    fn get_l2_to_l1_log_proof(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> RpcResult<Option<L2ToL1LogProof>>;

    #[method(name = "L1BatchNumber")]
    fn get_l1_batch_number(&self) -> RpcResult<U64>;

    #[method(name = "getL1BatchBlockRange")]
    fn get_miniblock_range(&self, batch: L1BatchNumber) -> RpcResult<Option<(U64, U64)>>;

    #[method(name = "getBlockDetails")]
    fn get_block_details(&self, block_number: MiniblockNumber) -> RpcResult<Option<BlockDetails>>;

    #[method(name = "getTransactionDetails")]
    fn get_transaction_details(&self, hash: H256) -> RpcResult<Option<TransactionDetails>>;
}
