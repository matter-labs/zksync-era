use crate::types::Token;
use bigdecimal::BigDecimal;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use std::collections::HashMap;
use zksync_types::api::{BridgeAddresses, L2ToL1LogProof, TransactionDetails};
use zksync_types::transaction_request::CallRequest;
use zksync_types::{
    api::U64,
    explorer_api::{BlockDetails, L1BatchDetails},
    fee::Fee,
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
    async fn estimate_fee(&self, req: CallRequest) -> RpcResult<Fee>;

    #[method(name = "estimateGasL1ToL2")]
    async fn estimate_gas_l1_to_l2(&self, req: CallRequest) -> RpcResult<U256>;

    #[method(name = "getMainContract")]
    async fn get_main_contract(&self) -> RpcResult<Address>;

    #[method(name = "getTestnetPaymaster")]
    async fn get_testnet_paymaster(&self) -> RpcResult<Option<Address>>;

    #[method(name = "getBridgeContracts")]
    async fn get_bridge_contracts(&self) -> RpcResult<BridgeAddresses>;

    #[method(name = "L1ChainId")]
    async fn l1_chain_id(&self) -> RpcResult<U64>;

    #[method(name = "getConfirmedTokens")]
    async fn get_confirmed_tokens(&self, from: u32, limit: u8) -> RpcResult<Vec<Token>>;
    #[method(name = "getTokenPrice")]
    async fn get_token_price(&self, token_address: Address) -> RpcResult<BigDecimal>;

    #[method(name = "getAllAccountBalances")]
    async fn get_all_account_balances(&self, address: Address)
        -> RpcResult<HashMap<Address, U256>>;

    #[method(name = "getL2ToL1MsgProof")]
    async fn get_l2_to_l1_msg_proof(
        &self,
        block: MiniblockNumber,
        sender: Address,
        msg: H256,
        l2_log_position: Option<usize>,
    ) -> RpcResult<Option<L2ToL1LogProof>>;

    #[method(name = "getL2ToL1LogProof")]
    async fn get_l2_to_l1_log_proof(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> RpcResult<Option<L2ToL1LogProof>>;

    #[method(name = "L1BatchNumber")]
    async fn get_l1_batch_number(&self) -> RpcResult<U64>;

    #[method(name = "getL1BatchBlockRange")]
    async fn get_miniblock_range(&self, batch: L1BatchNumber) -> RpcResult<Option<(U64, U64)>>;

    #[method(name = "getBlockDetails")]
    async fn get_block_details(
        &self,
        block_number: MiniblockNumber,
    ) -> RpcResult<Option<BlockDetails>>;

    #[method(name = "getTransactionDetails")]
    async fn get_transaction_details(&self, hash: H256) -> RpcResult<Option<TransactionDetails>>;

    #[method(name = "getRawBlockTransactions")]
    async fn get_raw_block_transactions(
        &self,
        block_number: MiniblockNumber,
    ) -> RpcResult<Vec<zksync_types::Transaction>>;

    #[method(name = "getL1BatchDetails")]
    async fn get_l1_batch_details(&self, batch: L1BatchNumber)
        -> RpcResult<Option<L1BatchDetails>>;

    #[method(name = "getBytecodeByHash")]
    async fn get_bytecode_by_hash(&self, hash: H256) -> RpcResult<Option<Vec<u8>>>;

    #[method(name = "getL1GasPrice")]
    async fn get_l1_gas_price(&self) -> RpcResult<U64>;
}
