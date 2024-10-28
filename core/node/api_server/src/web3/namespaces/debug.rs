use anyhow::Context as _;
use zksync_dal::{CoreDal, DalError};
use zksync_multivm::interface::{Call, CallType, ExecutionResult, OneshotTracingParams};
use zksync_system_constants::MAX_ENCODED_TX_SIZE;
use zksync_types::{
    api::{
        BlockId, BlockNumber, CallTracerBlockResult, CallTracerResult, DebugCall, DebugCallType,
        ResultDebugCall, SupportedTracers, TracerConfig,
    },
    debug_flat_call::{Action, CallResult, CallTraceMeta, DebugCallFlat, ResultDebugCallFlat},
    l2::L2Tx,
    transaction_request::CallRequest,
    web3, H256, U256,
};
use zksync_web3_decl::error::Web3Error;

use crate::{
    execution_sandbox::SandboxAction,
    web3::{backend_jsonrpsee::MethodTracer, state::RpcState},
};

#[derive(Debug, Clone)]
pub(crate) struct DebugNamespace {
    state: RpcState,
}

impl DebugNamespace {
    pub async fn new(state: RpcState) -> anyhow::Result<Self> {
        Ok(Self { state })
    }

    pub(crate) fn map_call(
        call: Call,
        meta: CallTraceMeta,
        tracer_option: TracerConfig,
    ) -> CallTracerResult {
        match tracer_option.tracer {
            SupportedTracers::CallTracer => CallTracerResult::CallTrace(Self::map_default_call(
                call,
                tracer_option.tracer_config.only_top_call,
            )),
            SupportedTracers::FlatCallTracer => {
                let mut calls = vec![];
                let mut traces = vec![meta.index_in_block];
                Self::flatten_call(
                    call,
                    &mut calls,
                    &mut traces,
                    tracer_option.tracer_config.only_top_call,
                    &meta,
                );
                CallTracerResult::FlatCallTrace(calls)
            }
        }
    }
    pub(crate) fn map_default_call(call: Call, only_top_call: bool) -> DebugCall {
        let calls = if only_top_call {
            vec![]
        } else {
            call.calls
                .into_iter()
                .map(|call| Self::map_default_call(call, false))
                .collect()
        };
        let debug_type = match call.r#type {
            CallType::Call(_) => DebugCallType::Call,
            CallType::Create => DebugCallType::Create,
            CallType::NearCall => unreachable!("We have to filter our near calls before"),
        };
        DebugCall {
            r#type: debug_type,
            from: call.from,
            to: call.to,
            gas: U256::from(call.gas),
            gas_used: U256::from(call.gas_used),
            value: call.value,
            output: web3::Bytes::from(call.output),
            input: web3::Bytes::from(call.input),
            error: call.error,
            revert_reason: call.revert_reason,
            calls,
        }
    }

    fn flatten_call(
        call: Call,
        calls: &mut Vec<DebugCallFlat>,
        trace_address: &mut Vec<usize>,
        only_top_call: bool,
        meta: &CallTraceMeta,
    ) {
        let subtraces = call.calls.len();
        let debug_type = match call.r#type {
            CallType::Call(_) => DebugCallType::Call,
            CallType::Create => DebugCallType::Create,
            CallType::NearCall => unreachable!("We have to filter our near calls before"),
        };

        let result = if call.error.is_none() {
            Some(CallResult {
                output: web3::Bytes::from(call.output),
                gas_used: U256::from(call.gas_used),
            })
        } else {
            None
        };

        calls.push(DebugCallFlat {
            action: Action {
                call_type: debug_type,
                from: call.from,
                to: call.to,
                gas: U256::from(call.gas),
                value: call.value,
                input: web3::Bytes::from(call.input),
            },
            result,
            subtraces,
            trace_address: trace_address.clone(), // Clone the current trace address
            transaction_position: meta.index_in_block,
            transaction_hash: meta.tx_hash,
            block_number: meta.block_number,
            block_hash: meta.block_hash,
            r#type: DebugCallType::Call,
        });

        if !only_top_call {
            for (number, call) in call.calls.into_iter().enumerate() {
                trace_address.push(number);
                Self::flatten_call(call, calls, trace_address, false, meta);
                trace_address.pop();
            }
        }
    }

    pub(crate) fn current_method(&self) -> &MethodTracer {
        &self.state.current_method
    }

    pub async fn debug_trace_block_impl(
        &self,
        block_id: BlockId,
        options: Option<TracerConfig>,
    ) -> Result<CallTracerBlockResult, Web3Error> {
        self.current_method().set_block_id(block_id);
        if matches!(block_id, BlockId::Number(BlockNumber::Pending)) {
            // See `EthNamespace::get_block_impl()` for an explanation why this check is needed.
            return Ok(CallTracerBlockResult::CallTrace(vec![]));
        }

        let mut connection = self.state.acquire_connection().await?;
        let block_number = self.state.resolve_block(&mut connection, block_id).await?;
        // let block_hash = block_hash self.state.
        self.current_method()
            .set_block_diff(self.state.last_sealed_l2_block.diff(block_number));

        let call_traces = connection
            .blocks_web3_dal()
            .get_traces_for_l2_block(block_number)
            .await
            .map_err(DalError::generalize)?;

        let options = options.unwrap_or_default();
        let result = match options.tracer {
            SupportedTracers::CallTracer => CallTracerBlockResult::CallTrace(
                call_traces
                    .into_iter()
                    .map(|(call, _)| ResultDebugCall {
                        result: Self::map_default_call(call, options.tracer_config.only_top_call),
                    })
                    .collect(),
            ),
            SupportedTracers::FlatCallTracer => {
                let res = call_traces
                    .into_iter()
                    .map(|(call, meta)| {
                        let mut traces = vec![meta.index_in_block];
                        let mut flat_calls = vec![];
                        Self::flatten_call(
                            call,
                            &mut flat_calls,
                            &mut traces,
                            options.tracer_config.only_top_call,
                            &meta,
                        );
                        ResultDebugCallFlat {
                            tx_hash: meta.tx_hash,
                            result: flat_calls,
                        }
                    })
                    .collect();
                CallTracerBlockResult::FlatCallTrace(res)
            }
        };
        Ok(result)
    }

    pub async fn debug_trace_transaction_impl(
        &self,
        tx_hash: H256,
        options: Option<TracerConfig>,
    ) -> Result<Option<CallTracerResult>, Web3Error> {
        let mut connection = self.state.acquire_connection().await?;
        let call_trace = connection
            .transactions_dal()
            .get_call_trace(tx_hash)
            .await
            .map_err(DalError::generalize)?;
        Ok(call_trace.map(|(call_trace, meta)| {
            Self::map_call(call_trace, meta, options.unwrap_or_default())
        }))
    }

    pub async fn debug_trace_call_impl(
        &self,
        mut request: CallRequest,
        block_id: Option<BlockId>,
        options: Option<TracerConfig>,
    ) -> Result<CallTracerResult, Web3Error> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        self.current_method().set_block_id(block_id);

        let options = options.unwrap_or_default();

        let mut connection = self.state.acquire_connection().await?;
        let block_args = self
            .state
            .resolve_block_args(&mut connection, block_id)
            .await?;
        self.current_method().set_block_diff(
            self.state
                .last_sealed_l2_block
                .diff_with_block_args(&block_args),
        );
        if request.gas.is_none() {
            request.gas = Some(block_args.default_eth_call_gas(&mut connection).await?);
        }

        let fee_input = if block_args.resolves_to_latest_sealed_l2_block() {
            // It is important to drop a DB connection before calling the provider, since it acquires a connection internally
            // on the main node.
            drop(connection);
            let scale_factor = self.state.api_config.estimate_gas_scale_factor;
            let fee_input_provider = &self.state.tx_sender.0.batch_fee_input_provider;
            // For now, the same scaling is used for both the L1 gas price and the pubdata price
            fee_input_provider
                .get_batch_fee_input_scaled(scale_factor, scale_factor)
                .await?
        } else {
            let fee_input = block_args.historical_fee_input(&mut connection).await?;
            drop(connection);
            fee_input
        };

        let call_overrides = request.get_call_overrides()?;
        let call = L2Tx::from_request(
            request.into(),
            MAX_ENCODED_TX_SIZE,
            block_args.use_evm_emulator(),
        )?;

        let vm_permit = self
            .state
            .tx_sender
            .vm_concurrency_limiter()
            .acquire()
            .await;
        let vm_permit = vm_permit.context("cannot acquire VM permit")?;

        // We don't need properly trace if we only need top call
        let tracing_params = OneshotTracingParams {
            trace_calls: !options.tracer_config.only_top_call,
        };

        let connection = self.state.acquire_connection().await?;
        let executor = &self.state.tx_sender.0.executor;
        let result = executor
            .execute_in_sandbox(
                vm_permit,
                connection,
                SandboxAction::Call {
                    call: call.clone(),
                    fee_input,
                    enforced_base_fee: call_overrides.enforced_base_fee,
                    tracing_params,
                },
                &block_args,
                None,
            )
            .await?;

        let (output, revert_reason) = match result.vm.result {
            ExecutionResult::Success { output, .. } => (output, None),
            ExecutionResult::Revert { output } => (vec![], Some(output.to_string())),
            ExecutionResult::Halt { reason } => {
                return Err(Web3Error::SubmitTransactionError(
                    reason.to_string(),
                    vec![],
                ))
            }
        };
        let call = Call::new_high_level(
            call.common_data.fee.gas_limit.as_u64(),
            result.vm.statistics.gas_used,
            call.execute.value,
            call.execute.calldata,
            output,
            revert_reason,
            result.call_traces,
        );
        let number = block_args.resolved_block_number();
        let meta = CallTraceMeta {
            block_number: number.0,
            // It's a call request, it's safe to everything as default
            ..Default::default()
        };
        Ok(Self::map_call(call, meta, options))
    }
}
