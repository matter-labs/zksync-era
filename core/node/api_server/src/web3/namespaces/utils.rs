use anyhow::Context as _;
use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_types::{
    api::{BlockId, BlockNumber},
    transaction_request::CallRequest,
};
use zksync_web3_decl::error::Web3Error;

use crate::{
    execution_sandbox::BlockArgs, tx_sender::SubmitTxError, web3::backend_jsonrpsee::MethodTracer,
};

/// Validates user-provided gas against the configured gas cap.
/// Returns an error if the user gas exceeds the effective limit.
pub async fn validate_gas_cap(
    request: &CallRequest,
    block_id: BlockId,
    block_args: &BlockArgs,
    connection: &mut Connection<'_, Core>,
    eth_call_gas_cap: Option<u64>,
    method_tracer: &MethodTracer,
) -> Result<(), Web3Error> {
    let Some(user_gas) = request.gas else {
        return Ok(()); // No user-provided gas to validate
    };

    let protocol_version = get_protocol_version_for_block(block_id, block_args, connection).await?;
    let effective_gas_limit =
        BlockArgs::calculate_effective_gas_limit(protocol_version, eth_call_gas_cap);

    if user_gas > effective_gas_limit.into() {
        return Err(method_tracer.map_submit_err(SubmitTxError::GasLimitIsTooBig));
    }

    Ok(())
}

/// Gets the protocol version for a given block, handling both pending and historical blocks.
pub async fn get_protocol_version_for_block(
    block_id: BlockId,
    block_args: &BlockArgs,
    connection: &mut Connection<'_, Core>,
) -> Result<zksync_types::ProtocolVersionId, Web3Error> {
    let is_pending = matches!(block_id, BlockId::Number(BlockNumber::Pending));

    if is_pending {
        Ok(connection.blocks_dal().pending_protocol_version().await?)
    } else {
        let block_number = block_args.resolved_block_number();
        let header = connection
            .blocks_dal()
            .get_l2_block_header(block_number)
            .await
            .map_err(DalError::generalize)?
            .with_context(|| format!("missing header for resolved block #{block_number}"))?;

        Ok(header
            .protocol_version
            .unwrap_or_else(|| zksync_types::ProtocolVersionId::last_potentially_undefined()))
    }
}
