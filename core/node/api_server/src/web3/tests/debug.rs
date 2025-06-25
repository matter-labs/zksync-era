//! Tests for the `debug` Web3 namespace.

use zksync_multivm::interface::{Call, TransactionExecutionResult};
use zksync_types::{
    api::{CallTracerConfig, SupportedTracers, TracerConfig},
    ExecuteTransactionCommon, BOOTLOADER_ADDRESS,
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    namespaces::DebugNamespaceClient,
};

use super::*;

fn execute_l2_transaction_with_traces(index_in_block: u8) -> TransactionExecutionResult {
    let first_call_trace = Call {
        from: Address::repeat_byte(index_in_block),
        to: Address::repeat_byte(index_in_block + 1),
        gas: 100,
        gas_used: 42,
        ..Call::default()
    };
    let second_call_trace = Call {
        from: Address::repeat_byte(0xff - index_in_block),
        to: Address::repeat_byte(0xab - index_in_block),
        value: 123.into(),
        gas: 58,
        gas_used: 10,
        input: b"input".to_vec(),
        output: b"output".to_vec(),
        ..Call::default()
    };
    TransactionExecutionResult {
        call_traces: vec![first_call_trace, second_call_trace],
        ..mock_execute_transaction(create_l2_transaction(1, 2).into())
    }
}

#[derive(Debug)]
struct TraceBlockTest(L2BlockNumber);

impl TestInit for TraceBlockTest {}

#[async_trait]
impl HttpTest for TraceBlockTest {
    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let tx_results = [0, 1, 2].map(execute_l2_transaction_with_traces);
        let mut storage = pool.connection().await?;
        let new_l2_block = store_l2_block(&mut storage, self.0, &tx_results).await?;
        drop(storage);

        let block_ids = [
            api::BlockId::Number((*self.0).into()),
            api::BlockId::Number(api::BlockNumber::Latest),
            api::BlockId::Hash(new_l2_block.hash),
        ];

        for block_id in block_ids {
            let block_traces = match block_id {
                api::BlockId::Number(number) => client.trace_block_by_number(number, None).await?,
                api::BlockId::Hash(hash) => client.trace_block_by_hash(hash, None).await?,
            }
            .unwrap_default();

            assert_eq!(block_traces.len(), tx_results.len()); // equals to the number of transactions in the block
            for (trace, tx_result) in block_traces.iter().zip(&tx_results) {
                let result = &trace.result;
                assert_eq!(result.from, Address::zero());
                assert_eq!(result.to, BOOTLOADER_ADDRESS);
                assert_eq!(result.gas, tx_result.transaction.gas_limit());
                let expected_calls: Vec<_> = tx_result
                    .call_traces
                    .iter()
                    .map(|call| DebugNamespace::map_default_call(call.clone(), false, None))
                    .collect();
                assert_eq!(result.calls, expected_calls);
            }
        }

        let missing_block_number = api::BlockNumber::from(*self.0 + 100);
        let error = client
            .trace_block_by_number(missing_block_number, None)
            .await
            .unwrap_err();
        if let ClientError::Call(error) = error {
            assert_eq!(error.code(), ErrorCode::InvalidParams.code());
            assert!(
                error.message().contains("Block") && error.message().contains("doesn't exist"),
                "{error:?}"
            );
            assert!(error.data().is_none(), "{error:?}");
        } else {
            panic!("Unexpected error: {error:?}");
        }

        Ok(())
    }
}

#[tokio::test]
async fn tracing_block() {
    test_http_server(TraceBlockTest(L2BlockNumber(1))).await;
}

#[derive(Debug)]
struct TraceBlockFlatTest(L2BlockNumber);

impl TestInit for TraceBlockFlatTest {}

#[async_trait]
impl HttpTest for TraceBlockFlatTest {
    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let tx_results = [0, 1, 2].map(execute_l2_transaction_with_traces);
        let mut storage = pool.connection().await?;
        store_l2_block(&mut storage, self.0, &tx_results).await?;
        drop(storage);

        let block_ids = [
            api::BlockId::Number((*self.0).into()),
            api::BlockId::Number(api::BlockNumber::Latest),
        ];

        for block_id in block_ids {
            if let api::BlockId::Number(number) = block_id {
                let block_traces = client
                    .trace_block_by_number(
                        number,
                        Some(TracerConfig {
                            tracer: SupportedTracers::FlatCallTracer,
                            tracer_config: CallTracerConfig {
                                only_top_call: false,
                            },
                        }),
                    )
                    .await?
                    .unwrap_flat();

                assert_eq!(block_traces.len(), tx_results.len());

                let tx_traces = &block_traces.first().unwrap().result;

                // First tx has 2 nested calls, thus 2 sub-traces
                assert_eq!(tx_traces[0].subtraces, 2);
                assert_eq!(tx_traces[0].trace_address, [0]);
                // Second flat-call (fist nested call) do not have nested calls
                assert_eq!(tx_traces[1].subtraces, 0);
                assert_eq!(tx_traces[1].trace_address, [0, 0]);

                let top_level_call_indexes = [0, 1, 2];
                let top_level_traces = top_level_call_indexes
                    .iter()
                    .map(|&i| block_traces[i].clone());

                for (top_level_trace, tx_result) in top_level_traces.zip(&tx_results) {
                    let trace = top_level_trace.result.first().unwrap();
                    assert_eq!(trace.action.from, Address::zero());
                    assert_eq!(trace.action.to, BOOTLOADER_ADDRESS);
                    assert_eq!(trace.action.gas, tx_result.transaction.gas_limit());
                }
                // TODO: test inner calls
            }
        }

        let missing_block_number = api::BlockNumber::from(*self.0 + 100);
        let error = client
            .trace_block_by_number(
                missing_block_number,
                Some(TracerConfig {
                    tracer: SupportedTracers::FlatCallTracer,
                    tracer_config: CallTracerConfig {
                        only_top_call: false,
                    },
                }),
            )
            .await
            .unwrap_err();
        if let ClientError::Call(error) = error {
            assert_eq!(error.code(), ErrorCode::InvalidParams.code());
            assert!(
                error.message().contains("Block") && error.message().contains("doesn't exist"),
                "{error:?}"
            );
            assert!(error.data().is_none(), "{error:?}");
        } else {
            panic!("Unexpected error: {error:?}");
        }

        Ok(())
    }
}

#[tokio::test]
async fn tracing_block_flat() {
    test_http_server(TraceBlockFlatTest(L2BlockNumber(1))).await;
}

#[derive(Debug)]
struct TraceTransactionTest;

impl TestInit for TraceTransactionTest {}

#[async_trait]
impl HttpTest for TraceTransactionTest {
    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let tx_results = [execute_l2_transaction_with_traces(0)];
        let mut storage = pool.connection().await?;
        store_l2_block(&mut storage, L2BlockNumber(1), &tx_results).await?;
        drop(storage);

        let expected_calls: Vec<_> = tx_results[0]
            .call_traces
            .iter()
            .map(|call| DebugNamespace::map_default_call(call.clone(), false, None))
            .collect();

        let result = client
            .trace_transaction(tx_results[0].hash, None)
            .await?
            .context("no transaction traces")?
            .unwrap_default();
        assert_eq!(result.from, Address::zero());
        assert_eq!(result.to, BOOTLOADER_ADDRESS);
        assert_eq!(result.gas, tx_results[0].transaction.gas_limit());
        assert_eq!(result.calls, expected_calls);

        Ok(())
    }
}

#[tokio::test]
async fn tracing_transaction() {
    test_http_server(TraceTransactionTest).await;
}

#[derive(Debug)]
struct TraceBlockTestWithSnapshotRecovery;

impl TestInit for TraceBlockTestWithSnapshotRecovery {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::empty_recovery()
    }
}

#[async_trait]
impl HttpTest for TraceBlockTestWithSnapshotRecovery {
    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let snapshot_l2_block_number = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK;
        let missing_l2_block_numbers = [
            L2BlockNumber(0),
            snapshot_l2_block_number - 1,
            snapshot_l2_block_number,
        ];

        for number in missing_l2_block_numbers {
            let error = client
                .trace_block_by_number(number.0.into(), None)
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, snapshot_l2_block_number + 1);
        }

        TraceBlockTest(snapshot_l2_block_number + 2)
            .test(client, pool)
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn tracing_block_after_snapshot_recovery() {
    test_http_server(TraceBlockTestWithSnapshotRecovery).await;
}

#[derive(Debug)]
struct GetRawTransactionTest;

impl TestInit for GetRawTransactionTest {}

#[async_trait]
impl HttpTest for GetRawTransactionTest {
    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let tx_results = [execute_l2_transaction_with_traces(0)];
        let mut storage = pool.connection().await?;
        store_l2_block(&mut storage, L2BlockNumber(1), &tx_results).await?;
        drop(storage);

        let result = client
            .get_raw_transaction(tx_results[0].hash)
            .await?
            .context("no raw transaction")?;
        let ExecuteTransactionCommon::L2(common_data) = &tx_results[0].transaction.common_data
        else {
            panic!("non-L2 tx")
        };
        assert_eq!(result.0, common_data.input.as_ref().unwrap().data);

        Ok(())
    }
}

#[tokio::test]
async fn get_raw_transaction() {
    test_http_server(GetRawTransactionTest).await;
}

#[derive(Debug)]
struct GetRawTransactionsTest(L2BlockNumber);

impl TestInit for GetRawTransactionsTest {}

#[async_trait]
impl HttpTest for GetRawTransactionsTest {
    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let tx_results = [0, 1, 2].map(execute_l2_transaction_with_traces);
        let mut storage = pool.connection().await?;
        let new_l2_block = store_l2_block(&mut storage, self.0, &tx_results).await?;
        drop(storage);

        let block_ids = [
            api::BlockId::Number((*self.0).into()),
            api::BlockId::Number(BlockNumber::Latest),
            api::BlockId::Hash(new_l2_block.hash),
        ];
        for block_id in block_ids {
            let raw_transactions = client
                .get_raw_transactions(block_id)
                .await
                .context("no raw transactions")?;
            assert_eq!(raw_transactions.len(), tx_results.len()); // equals to the number of transactions in the block
            for (raw_transaction, tx_result) in raw_transactions.iter().zip(&tx_results) {
                let ExecuteTransactionCommon::L2(common_data) = &tx_result.transaction.common_data
                else {
                    panic!("non-L2 tx")
                };
                assert_eq!(raw_transaction.0, common_data.input.as_ref().unwrap().data);
            }
        }

        let missing_block_number = api::BlockNumber::from(*self.0 + 100);
        let error = client
            .get_raw_transactions(api::BlockId::Number(missing_block_number))
            .await
            .unwrap_err();
        if let ClientError::Call(error) = error {
            assert_eq!(error.code(), ErrorCode::InvalidParams.code());
            assert!(
                error.message().contains("Block") && error.message().contains("doesn't exist"),
                "{error:?}"
            );
            assert!(error.data().is_none(), "{error:?}");
        } else {
            panic!("Unexpected error: {error:?}");
        }

        Ok(())
    }
}

#[tokio::test]
async fn get_raw_transactions() {
    test_http_server(GetRawTransactionsTest(L2BlockNumber(1))).await;
}
