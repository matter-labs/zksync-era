// use async_trait::async_trait;
// use zk_os_forward_system::run::{ExecutionResult, PreimageSource};
// use zksync_web3_decl::jsonrpsee::core::RpcResult;
// use zksync_web3_decl::types::{Block, Bytes, Filter, FilterChanges, Index, Log, SyncState, TransactionReceipt, U256, U64, U64Number};
// use crate::{CHAIN_ID, MAX_TX_SIZE};
// use crate::storage::in_memory_state::{InMemoryStorage};
// use zksync_zkos_vm_runner::zkos_conversions::{h256_to_bytes32, tx_abi_encode};
//
// use zksync_web3_decl::{namespaces::EthNamespaceServer};
//
// use zksync_types::{api::{
//     state_override::StateOverride, BlockId, BlockIdVariant, BlockNumber, FeeHistory, Transaction,
//     TransactionVariant,
// }, transaction_request::CallRequest, Address, H256, api, L2ChainId};
// use zksync_types::l2::L2Tx;
// use zksync_web3_decl::jsonrpsee::types::{ErrorCode, ErrorObject};
// use zksync_web3_decl::jsonrpsee::types::error::INTERNAL_ERROR_CODE;
// use crate::api::eth_impl::EthNamespace;
// use crate::api::resolve_block_id;
// use crate::execution::sandbox::execute;
// use crate::mempool::Mempool;
// use crate::storage::block_replay_storage::BlockReplayStorage;
// use crate::storage::StateHandle;
//
// #[async_trait]
// impl EthNamespaceServer for EthNamespace {
//     async fn get_block_number(&self) -> RpcResult<U64> {
//         self.get_block_number_impl()
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn chain_id(&self) -> RpcResult<U64> {
//         Ok(self.chain_id_impl())
//     }
//
//     async fn call(
//         &self,
//         req: CallRequest,
//         block: Option<BlockIdVariant>,
//         state_override: Option<StateOverride>,
//     ) -> RpcResult<Bytes> {
//         self.call_impl(req, block.map(Into::into), state_override)
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn estimate_gas(
//         &self,
//         req: CallRequest,
//         block: Option<BlockNumber>,
//         state_override: Option<StateOverride>,
//     ) -> RpcResult<U256> {
//         self.estimate_gas_mock(req, block, state_override)
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn gas_price(&self) -> RpcResult<U256> {
//         self.gas_price_mock()
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn new_filter(&self, filter: Filter) -> RpcResult<U256> {
//         unimplemented!("new_filter is not implemented");
//     }
//
//     async fn new_block_filter(&self) -> RpcResult<U256> {
//         unimplemented!("new_block_filter is not implemented");
//     }
//
//     async fn uninstall_filter(&self, idx: U256) -> RpcResult<bool> {
//         unimplemented!("uninstall_filter is not implemented");
//     }
//
//     async fn new_pending_transaction_filter(&self) -> RpcResult<U256> {
//         unimplemented!("not implemented");
//     }
//
//     async fn get_logs(&self, filter: Filter) -> RpcResult<Vec<Log>> {
//         unimplemented!("not implemented");
//     }
//
//     async fn get_filter_logs(&self, filter_index: U256) -> RpcResult<FilterChanges> {
//         unimplemented!("not implemented");
//     }
//
//     async fn get_filter_changes(&self, filter_index: U256) -> RpcResult<FilterChanges> {
//         unimplemented!("not implemented");
//     }
//
//     async fn get_balance(
//         &self,
//         address: Address,
//         block: Option<BlockIdVariant>,
//     ) -> RpcResult<U256> {
//         self.get_balance_impl(address, block.map(Into::into))
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn get_block_by_number(
//         &self,
//         block_number: BlockNumber,
//         full_transactions: bool,
//     ) -> RpcResult<Option<Block<TransactionVariant>>> {
//         unimplemented!("not implemented");
//     }
//
//     async fn get_block_by_hash(
//         &self,
//         hash: H256,
//         full_transactions: bool,
//     ) -> RpcResult<Option<Block<TransactionVariant>>> {
//         unimplemented!("not implemented");
//     }
//
//     async fn get_block_transaction_count_by_number(
//         &self,
//         block_number: BlockNumber,
//     ) -> RpcResult<Option<U256>> {
//         unimplemented!("not implemented");
//     }
//
//     async fn get_block_receipts(
//         &self,
//         block_id: BlockId,
//     ) -> RpcResult<Option<Vec<TransactionReceipt>>> {
//         unimplemented!("not implemented");
//     }
//
//     async fn get_block_transaction_count_by_hash(
//         &self,
//         block_hash: H256,
//     ) -> RpcResult<Option<U256>> {
//         unimplemented!("not implemented");
//     }
//
//     async fn get_code(&self, address: Address, block: Option<BlockIdVariant>) -> RpcResult<Bytes> {
//         unimplemented!("not implemented");
//     }
//
//     async fn get_storage_at(
//         &self,
//         address: Address,
//         idx: U256,
//         block: Option<BlockIdVariant>,
//     ) -> RpcResult<H256>  {
//         unimplemented!("not implemented");
//     }
//
//     async fn get_transaction_count(
//         &self,
//         address: Address,
//         block: Option<BlockIdVariant>,
//     ) -> RpcResult<U256> {
//         self.get_transaction_count_impl(address, block.map(Into::into))
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn get_transaction_by_hash(&self, hash: H256) -> RpcResult<Option<Transaction>> {
//         self.get_transaction_impl(hash)
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn get_transaction_by_block_hash_and_index(
//         &self,
//         block_hash: H256,
//         index: Index,
//     ) -> RpcResult<Option<Transaction>> {
//         self.get_transaction_impl(TransactionId::Block(BlockId::Hash(block_hash), index))
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn get_transaction_by_block_number_and_index(
//         &self,
//         block_number: BlockNumber,
//         index: Index,
//     ) -> RpcResult<Option<Transaction>> {
//         self.get_transaction_impl(TransactionId::Block(BlockId::Number(block_number), index))
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn get_transaction_receipt(&self, hash: H256) -> RpcResult<Option<TransactionReceipt>> {
//         self.get_transaction_receipt_impl(hash)
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn protocol_version(&self) -> RpcResult<String> {
//         Ok(self.protocol_version())
//     }
//
//     async fn send_raw_transaction(&self, tx_bytes: Bytes) -> RpcResult<H256> {
//         self.send_raw_transaction_impl(tx_bytes)
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn syncing(&self) -> RpcResult<SyncState> {
//         Ok(self.syncing_impl())
//     }
//
//     async fn accounts(&self) -> RpcResult<Vec<Address>> {
//         Ok(self.accounts_impl())
//     }
//
//     async fn coinbase(&self) -> RpcResult<Address> {
//         Ok(self.coinbase_impl())
//     }
//
//     async fn compilers(&self) -> RpcResult<Vec<String>> {
//         Ok(self.compilers_impl())
//     }
//
//     async fn hashrate(&self) -> RpcResult<U256> {
//         Ok(self.hashrate_impl())
//     }
//
//     async fn get_uncle_count_by_block_hash(&self, hash: H256) -> RpcResult<Option<U256>> {
//         Ok(self.uncle_count_impl(BlockId::Hash(hash)))
//     }
//
//     async fn get_uncle_count_by_block_number(
//         &self,
//         number: BlockNumber,
//     ) -> RpcResult<Option<U256>> {
//         Ok(self.uncle_count_impl(BlockId::Number(number)))
//     }
//
//     async fn mining(&self) -> RpcResult<bool> {
//         Ok(self.mining_impl())
//     }
//
//     async fn fee_history(
//         &self,
//         block_count: U64Number,
//         newest_block: BlockNumber,
//         reward_percentiles: Option<Vec<f32>>,
//     ) -> RpcResult<FeeHistory> {
//         self.fee_history_impl(
//             block_count.into(),
//             newest_block,
//             reward_percentiles,
//         )
//             .await
//             .map_err(|err| self.map_err(err))
//     }
//
//     async fn max_priority_fee_per_gas(&self) -> RpcResult<U256> {
//         Ok(self.max_priority_fee_per_gas_impl())
//     }
// }