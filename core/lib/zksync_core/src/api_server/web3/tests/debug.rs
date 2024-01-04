//! Tests for the `debug` Web3 namespace.

use zksync_types::{tx::TransactionExecutionResult, vm_trace::Call, BOOTLOADER_ADDRESS};
use zksync_web3_decl::namespaces::DebugNamespaceClient;

use super::*;

fn execute_l2_transaction_with_traces() -> TransactionExecutionResult {
    let first_call_trace = Call {
        from: Address::repeat_byte(1),
        to: Address::repeat_byte(2),
        gas: 100,
        gas_used: 42,
        ..Call::default()
    };
    let second_call_trace = Call {
        from: Address::repeat_byte(0xff),
        to: Address::repeat_byte(0xab),
        value: 123.into(),
        gas: 58,
        gas_used: 10,
        input: b"input".to_vec(),
        output: b"output".to_vec(),
        ..Call::default()
    };
    TransactionExecutionResult {
        call_traces: vec![first_call_trace, second_call_trace],
        ..execute_l2_transaction()
    }
}

#[derive(Debug)]
struct TraceBlockTest;

#[async_trait]
impl HttpTest for TraceBlockTest {
    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let tx_results = [execute_l2_transaction_with_traces()];
        let mut storage = pool.access_storage().await?;
        let new_miniblock = store_miniblock(&mut storage, &tx_results).await?;
        drop(storage);

        let block_ids = [
            api::BlockId::Number(1.into()),
            api::BlockId::Number(api::BlockNumber::Latest),
            api::BlockId::Hash(new_miniblock.hash),
        ];
        let expected_calls: Vec<_> = tx_results[0]
            .call_traces
            .iter()
            .map(|call| api::DebugCall::from(call.clone()))
            .collect();

        for block_id in block_ids {
            let block_traces = match block_id {
                api::BlockId::Number(number) => client.trace_block_by_number(number, None).await?,
                api::BlockId::Hash(hash) => client.trace_block_by_hash(hash, None).await?,
            };

            assert_eq!(block_traces.len(), 1); // equals to the number of transactions in the block
            let api::ResultDebugCall { result } = &block_traces[0];
            assert_eq!(result.from, Address::zero());
            assert_eq!(result.to, BOOTLOADER_ADDRESS);
            assert_eq!(result.gas, tx_results[0].transaction.gas_limit());
            assert_eq!(result.calls, expected_calls);
        }
        Ok(())
    }
}

#[tokio::test]
async fn tracing_block() {
    test_http_server(TraceBlockTest).await;
}

#[derive(Debug)]
struct TraceTransactionTest;

#[async_trait]
impl HttpTest for TraceTransactionTest {
    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let tx_results = [execute_l2_transaction_with_traces()];
        let mut storage = pool.access_storage().await?;
        store_miniblock(&mut storage, &tx_results).await?;
        drop(storage);

        let expected_calls: Vec<_> = tx_results[0]
            .call_traces
            .iter()
            .map(|call| api::DebugCall::from(call.clone()))
            .collect();

        let result = client
            .trace_transaction(tx_results[0].hash, None)
            .await?
            .context("no transaction traces")?;
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
