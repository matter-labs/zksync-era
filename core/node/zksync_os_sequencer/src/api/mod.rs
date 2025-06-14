mod eth;
mod eth_impl;
mod api_tx_operations;

use anyhow::Context;
use zksync_types::api::{BlockId, BlockIdVariant, BlockNumber};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::EthNamespaceServer,
    types::{Filter, FilterChanges},
};
use zksync_web3_decl::jsonrpsee::RpcModule;
use zksync_web3_decl::jsonrpsee::server::{RpcServiceBuilder, ServerBuilder};
use crate::{BLOCKS_TO_RETAIN, JSON_RPC_ADDR};
use crate::api::eth_impl::EthNamespace;
use crate::mempool::Mempool;
use crate::storage::block_replay_storage::BlockReplayStorage;
use crate::storage::in_memory_state::InMemoryStorage;
use crate::storage::StateHandle;

// stripped-down version of `api_server/src/web3/mod.rs`
pub async fn run_jsonrpsee_server(
    state_handle: StateHandle,
    mempool: Mempool,
    block_replay_storage: BlockReplayStorage,
) -> anyhow::Result<()> {
    tracing::info!("Starting JSON-RPC server at {}", JSON_RPC_ADDR);

    let mut rpc = RpcModule::new(());
    rpc.merge(EthNamespace::new(state_handle, mempool, block_replay_storage).into_rpc())?;

    let server_builder = ServerBuilder::default();
    // .max_connections(max_connections as u32)
    // .set_http_middleware(middleware)
    // .max_response_body_size(response_body_size_limit)
    // .set_batch_request_config(batch_request_config)
    // .set_rpc_middleware(rpc_middleware);

    let server = server_builder
        .http_only()
        .build(JSON_RPC_ADDR)
        .await
        .context("Failed building HTTP JSON-RPC server")?;

    let server_handle = server.start(rpc);
    tracing::info!("Started JSON-RPC server at {}", JSON_RPC_ADDR);

    Ok(server_handle.stopped().await)
}


pub fn resolve_block_id(
    block: Option<BlockIdVariant>,
    state_handle: StateHandle,
) -> u64 {
    let block_id: BlockId = block
        .map(|b| b.into())
        .unwrap_or_else(|| BlockId::Number(BlockNumber::Pending));

    match block_id {
        BlockId::Hash(_) => unimplemented!(),
        BlockId::Number(BlockNumber::Pending) |
        BlockId::Number(BlockNumber::Committed) |
        BlockId::Number(BlockNumber::Finalized) |
        BlockId::Number(BlockNumber::Latest) |
        BlockId::Number(BlockNumber::L1Committed) =>
            state_handle.last_canonized_block_number(),
        BlockId::Number(BlockNumber::Earliest) => unimplemented!(),
        BlockId::Number(BlockNumber::Number(number)) => {
            let current_number = state_handle.last_canonized_block_number();
            if number.as_u64() <= current_number - (BLOCKS_TO_RETAIN as u64) {
                tracing::warn!(
                    "Requested block number {} is too low, using latest block instead",
                    number
                );
                unimplemented!()
            }
            number.as_u64()
        }
    }
}