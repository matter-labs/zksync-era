use std::collections::HashMap;

#[cfg_attr(not(feature = "server"), allow(unused_imports))]
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use zksync_types::{
    api::{
        state_override::StateOverride, BlockDetails, BridgeAddresses, L1BatchDetails,
        L2ToL1LogProof, Proof, ProtocolVersion, TransactionDetails,
    },
    fee::Fee,
    fee_model::{FeeParams, PubdataIndependentBatchFeeModelInput},
    transaction_request::CallRequest,
    Address, L1BatchNumber, L2BlockNumber, H256, U256, U64,
};

use crate::{
    client::{ForWeb3Network, L2},
    types::Token,
};

#[cfg_attr(
    feature = "server",
    rpc(server, client, namespace = "zks", client_bounds(Self: ForWeb3Network<Net = L2>))
)]
#[cfg_attr(
    not(feature = "server"),
    rpc(client, namespace = "zks", client_bounds(Self: ForWeb3Network<Net = L2>))
)]
pub trait ZksNamespace {
    #[method(name = "estimateFee")]
    async fn estimate_fee(
        &self,
        req: CallRequest,
        state_override: Option<StateOverride>,
    ) -> RpcResult<Fee>;

    #[method(name = "estimateGasL1ToL2")]
    async fn estimate_gas_l1_to_l2(
        &self,
        req: CallRequest,
        state_override: Option<StateOverride>,
    ) -> RpcResult<U256>;

    #[method(name = "getBridgehubContract")]
    async fn get_bridgehub_contract(&self) -> RpcResult<Option<Address>>;

    #[method(name = "getMainContract")]
    async fn get_main_l1_contract(&self) -> RpcResult<Address>;

    #[method(name = "getL2Multicall3")]
    async fn get_l2_multicall3(&self) -> RpcResult<Option<Address>>;

    #[method(name = "getTestnetPaymaster")]
    async fn get_testnet_paymaster(&self) -> RpcResult<Option<Address>>;

    #[method(name = "getTimestampAsserter")]
    async fn get_timestamp_asserter(&self) -> RpcResult<Option<Address>>;

    #[method(name = "getL2DAValidator")]
    async fn get_l2_da_validator(&self) -> RpcResult<Option<Address>>;

    #[method(name = "getBridgeContracts")]
    async fn get_bridge_contracts(&self) -> RpcResult<BridgeAddresses>;

    #[method(name = "getBaseTokenL1Address")]
    async fn get_base_token_l1_address(&self) -> RpcResult<Address>;

    #[method(name = "L1ChainId")]
    async fn l1_chain_id(&self) -> RpcResult<U64>;

    #[method(name = "getConfirmedTokens")]
    async fn get_confirmed_tokens(&self, from: u32, limit: u8) -> RpcResult<Vec<Token>>;

    #[method(name = "getAllAccountBalances")]
    async fn get_all_account_balances(&self, address: Address)
        -> RpcResult<HashMap<Address, U256>>;

    #[method(name = "getL2ToL1LogProof")]
    async fn get_l2_to_l1_log_proof(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> RpcResult<Option<L2ToL1LogProof>>;

    #[method(name = "L1BatchNumber")]
    async fn get_l1_batch_number(&self) -> RpcResult<U64>;

    #[method(name = "getL1BatchBlockRange")]
    async fn get_l2_block_range(&self, batch: L1BatchNumber) -> RpcResult<Option<(U64, U64)>>;

    #[method(name = "getBlockDetails")]
    async fn get_block_details(
        &self,
        block_number: L2BlockNumber,
    ) -> RpcResult<Option<BlockDetails>>;

    #[method(name = "getTransactionDetails")]
    async fn get_transaction_details(&self, hash: H256) -> RpcResult<Option<TransactionDetails>>;

    #[method(name = "getRawBlockTransactions")]
    async fn get_raw_block_transactions(
        &self,
        block_number: L2BlockNumber,
    ) -> RpcResult<Vec<zksync_types::Transaction>>;

    #[method(name = "getL1BatchDetails")]
    async fn get_l1_batch_details(&self, batch: L1BatchNumber)
        -> RpcResult<Option<L1BatchDetails>>;

    #[method(name = "getBytecodeByHash")]
    async fn get_bytecode_by_hash(&self, hash: H256) -> RpcResult<Option<Vec<u8>>>;

    #[method(name = "getL1GasPrice")]
    async fn get_l1_gas_price(&self) -> RpcResult<U64>;

    #[method(name = "getFeeParams")]
    async fn get_fee_params(&self) -> RpcResult<FeeParams>;

    // TODO: remove in favour of `en_getProtocolVersionInfo` once all ENs have been upgraded.
    #[deprecated]
    #[method(name = "getProtocolVersion")]
    async fn get_protocol_version(
        &self,
        version_id: Option<u16>,
    ) -> RpcResult<Option<ProtocolVersion>>;

    #[method(name = "getProof")]
    async fn get_proof(
        &self,
        address: Address,
        keys: Vec<H256>,
        l1_batch_number: L1BatchNumber,
    ) -> RpcResult<Option<Proof>>;

    #[method(name = "getBatchFeeInput")]
    async fn get_batch_fee_input(&self) -> RpcResult<PubdataIndependentBatchFeeModelInput>;
}
