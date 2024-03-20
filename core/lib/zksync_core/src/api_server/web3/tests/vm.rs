//! Tests for the VM-instantiating methods (e.g., `eth_call`).

use std::sync::atomic::{AtomicU32, Ordering};

use multivm::interface::{ExecutionResult, VmRevertReason};
use zksync_types::{
    get_intrinsic_constants, transaction_request::CallRequest, L2ChainId, PackedEthSignature, U256,
};
use zksync_utils::u256_to_h256;
use zksync_web3_decl::namespaces::DebugNamespaceClient;

use super::*;

#[derive(Debug)]
struct CallTest;

impl CallTest {
    fn call_request(data: &[u8]) -> CallRequest {
        CallRequest {
            from: Some(Address::repeat_byte(1)),
            to: Some(Address::repeat_byte(2)),
            data: Some(data.to_vec().into()),
            value: Some(4321.into()),
            gas: Some(123.into()),
            ..CallRequest::default()
        }
    }

    fn create_executor(only_block: MiniblockNumber) -> MockTransactionExecutor {
        let mut tx_executor = MockTransactionExecutor::default();
        tx_executor.set_call_responses(move |tx, block_args| {
            let expected_block_number = match tx.execute.calldata() {
                b"pending" => only_block + 1,
                b"first" => only_block,
                data => panic!("Unexpected calldata: {data:?}"),
            };
            assert_eq!(block_args.resolved_block_number(), expected_block_number);

            ExecutionResult::Success {
                output: b"output".to_vec(),
            }
        });
        tx_executor
    }
}

#[async_trait]
impl HttpTest for CallTest {
    fn transaction_executor(&self) -> MockTransactionExecutor {
        Self::create_executor(MiniblockNumber(0))
    }

    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let call_result = client.call(Self::call_request(b"pending"), None).await?;
        assert_eq!(call_result.0, b"output");

        let valid_block_numbers_and_calldata = [
            (api::BlockNumber::Pending, b"pending" as &[_]),
            (api::BlockNumber::Latest, b"first"),
            (0.into(), b"first"),
        ];
        for (number, calldata) in valid_block_numbers_and_calldata {
            let number = api::BlockIdVariant::BlockNumber(number);
            let call_result = client
                .call(Self::call_request(calldata), Some(number))
                .await?;
            assert_eq!(call_result.0, b"output");
        }

        let invalid_block_number = api::BlockNumber::from(100);
        let number = api::BlockIdVariant::BlockNumber(invalid_block_number);
        let error = client
            .call(Self::call_request(b"100"), Some(number))
            .await
            .unwrap_err();
        if let ClientError::Call(error) = error {
            assert_eq!(error.code(), ErrorCode::InvalidParams.code());
        } else {
            panic!("Unexpected error: {error:?}");
        }

        Ok(())
    }
}

#[tokio::test]
async fn call_method_basics() {
    test_http_server(CallTest).await;
}

#[derive(Debug)]
struct CallTestAfterSnapshotRecovery;

#[async_trait]
impl HttpTest for CallTestAfterSnapshotRecovery {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::empty_recovery()
    }

    fn transaction_executor(&self) -> MockTransactionExecutor {
        let first_local_miniblock = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        CallTest::create_executor(first_local_miniblock)
    }

    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let call_result = client
            .call(CallTest::call_request(b"pending"), None)
            .await?;
        assert_eq!(call_result.0, b"output");
        let pending_block_number = api::BlockIdVariant::BlockNumber(api::BlockNumber::Pending);
        let call_result = client
            .call(
                CallTest::call_request(b"pending"),
                Some(pending_block_number),
            )
            .await?;
        assert_eq!(call_result.0, b"output");

        let first_local_miniblock = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        let pruned_block_numbers = [0, 1, StorageInitialization::SNAPSHOT_RECOVERY_BLOCK.0];
        for number in pruned_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number.into());
            let error = client
                .call(CallTest::call_request(b"pruned"), Some(number))
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, first_local_miniblock);
        }

        let first_miniblock_numbers = [api::BlockNumber::Latest, first_local_miniblock.0.into()];
        for number in first_miniblock_numbers {
            let number = api::BlockIdVariant::BlockNumber(number);
            let call_result = client
                .call(CallTest::call_request(b"first"), Some(number))
                .await?;
            assert_eq!(call_result.0, b"output");
        }
        Ok(())
    }
}

#[tokio::test]
async fn call_method_after_snapshot_recovery() {
    test_http_server(CallTestAfterSnapshotRecovery).await;
}

#[derive(Debug)]
struct SendRawTransactionTest {
    snapshot_recovery: bool,
}

impl SendRawTransactionTest {
    fn transaction_bytes_and_hash() -> (Vec<u8>, H256) {
        let (private_key, address) = Self::private_key_and_address();
        let tx_request = api::TransactionRequest {
            chain_id: Some(L2ChainId::default().as_u64()),
            from: Some(address),
            to: Some(Address::repeat_byte(2)),
            value: 123_456.into(),
            gas: (get_intrinsic_constants().l2_tx_intrinsic_gas * 2).into(),
            gas_price: StateKeeperConfig::for_tests().minimal_l2_gas_price.into(),
            input: vec![1, 2, 3, 4].into(),
            ..api::TransactionRequest::default()
        };
        let mut rlp = Default::default();
        tx_request.rlp(&mut rlp, L2ChainId::default().as_u64(), None);
        let data = rlp.out();
        let signed_message = PackedEthSignature::message_to_signed_bytes(&data);
        let signature = PackedEthSignature::sign_raw(&private_key, &signed_message).unwrap();

        let mut rlp = Default::default();
        tx_request.rlp(&mut rlp, L2ChainId::default().as_u64(), Some(&signature));
        let data = rlp.out();
        let (_, tx_hash) =
            api::TransactionRequest::from_bytes(&data, L2ChainId::default()).unwrap();
        (data.into(), tx_hash)
    }

    fn private_key_and_address() -> (H256, Address) {
        let private_key = H256::repeat_byte(11);
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();
        (private_key, address)
    }

    fn balance_storage_log() -> StorageLog {
        let (_, address) = Self::private_key_and_address();
        let balance_key = storage_key_for_eth_balance(&address);
        StorageLog::new_write_log(balance_key, u256_to_h256(U256::one() << 64))
    }
}

#[async_trait]
impl HttpTest for SendRawTransactionTest {
    fn storage_initialization(&self) -> StorageInitialization {
        if self.snapshot_recovery {
            let logs = vec![Self::balance_storage_log()];
            StorageInitialization::Recovery {
                logs,
                factory_deps: HashMap::default(),
            }
        } else {
            StorageInitialization::Genesis
        }
    }

    fn transaction_executor(&self) -> MockTransactionExecutor {
        let mut tx_executor = MockTransactionExecutor::default();
        let pending_block = if self.snapshot_recovery {
            StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 2
        } else {
            MiniblockNumber(1)
        };
        tx_executor.set_tx_responses(move |tx, block_args| {
            assert_eq!(tx.hash(), Self::transaction_bytes_and_hash().1);
            assert_eq!(block_args.resolved_block_number(), pending_block);
            ExecutionResult::Success { output: vec![] }
        });
        tx_executor
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        if !self.snapshot_recovery {
            // Manually set sufficient balance for the transaction account.
            let mut storage = pool.connection().await?;
            storage
                .storage_logs_dal()
                .append_storage_logs(
                    MiniblockNumber(0),
                    &[(H256::zero(), vec![Self::balance_storage_log()])],
                )
                .await?;
        }

        let (tx_bytes, tx_hash) = Self::transaction_bytes_and_hash();
        let send_result = client.send_raw_transaction(tx_bytes.into()).await?;
        assert_eq!(send_result, tx_hash);
        Ok(())
    }
}

#[tokio::test]
async fn send_raw_transaction_basics() {
    test_http_server(SendRawTransactionTest {
        snapshot_recovery: false,
    })
    .await;
}

#[tokio::test]
async fn send_raw_transaction_after_snapshot_recovery() {
    test_http_server(SendRawTransactionTest {
        snapshot_recovery: true,
    })
    .await;
}

#[derive(Debug)]
struct TraceCallTest;

impl TraceCallTest {
    fn assert_debug_call(call_request: &CallRequest, call_result: &api::DebugCall) {
        assert_eq!(call_result.from, Address::zero());
        assert_eq!(call_result.gas, call_request.gas.unwrap());
        assert_eq!(call_result.value, call_request.value.unwrap());
        assert_eq!(
            call_result.input,
            call_request
                .clone()
                .input
                .or(call_request.clone().data)
                .unwrap()
        );
        assert_eq!(call_result.output.0, b"output");
    }
}

#[async_trait]
impl HttpTest for TraceCallTest {
    fn transaction_executor(&self) -> MockTransactionExecutor {
        CallTest::create_executor(MiniblockNumber(0))
    }

    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let call_request = CallTest::call_request(b"pending");
        let call_result = client.trace_call(call_request.clone(), None, None).await?;
        Self::assert_debug_call(&call_request, &call_result);
        let pending_block_number = api::BlockId::Number(api::BlockNumber::Pending);
        let call_result = client
            .trace_call(call_request.clone(), Some(pending_block_number), None)
            .await?;
        Self::assert_debug_call(&call_request, &call_result);

        let genesis_block_numbers = [
            api::BlockNumber::Earliest,
            api::BlockNumber::Latest,
            0.into(),
        ];
        let call_request = CallTest::call_request(b"first");
        for number in genesis_block_numbers {
            let call_result = client
                .trace_call(
                    call_request.clone(),
                    Some(api::BlockId::Number(number)),
                    None,
                )
                .await?;
            Self::assert_debug_call(&call_request, &call_result);
        }

        let invalid_block_number = api::BlockNumber::from(100);
        let error = client
            .trace_call(
                CallTest::call_request(b"100"),
                Some(api::BlockId::Number(invalid_block_number)),
                None,
            )
            .await
            .unwrap_err();
        if let ClientError::Call(error) = error {
            assert_eq!(error.code(), ErrorCode::InvalidParams.code());
        } else {
            panic!("Unexpected error: {error:?}");
        }

        Ok(())
    }
}

#[tokio::test]
async fn trace_call_basics() {
    test_http_server(TraceCallTest).await;
}

#[derive(Debug)]
struct TraceCallTestAfterSnapshotRecovery;

#[async_trait]
impl HttpTest for TraceCallTestAfterSnapshotRecovery {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::empty_recovery()
    }

    fn transaction_executor(&self) -> MockTransactionExecutor {
        let number = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        CallTest::create_executor(number)
    }

    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let call_request = CallTest::call_request(b"pending");
        let call_result = client.trace_call(call_request.clone(), None, None).await?;
        TraceCallTest::assert_debug_call(&call_request, &call_result);
        let pending_block_number = api::BlockId::Number(api::BlockNumber::Pending);
        let call_result = client
            .trace_call(call_request.clone(), Some(pending_block_number), None)
            .await?;
        TraceCallTest::assert_debug_call(&call_request, &call_result);

        let first_local_miniblock = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        let pruned_block_numbers = [0, 1, StorageInitialization::SNAPSHOT_RECOVERY_BLOCK.0];
        for number in pruned_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number.into());
            let error = client
                .call(CallTest::call_request(b"pruned"), Some(number))
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, first_local_miniblock);
        }

        let call_request = CallTest::call_request(b"first");
        let first_miniblock_numbers = [api::BlockNumber::Latest, first_local_miniblock.0.into()];
        for number in first_miniblock_numbers {
            let number = api::BlockId::Number(number);
            let call_result = client
                .trace_call(call_request.clone(), Some(number), None)
                .await?;
            TraceCallTest::assert_debug_call(&call_request, &call_result);
        }
        Ok(())
    }
}

#[tokio::test]
async fn trace_call_after_snapshot_recovery() {
    test_http_server(TraceCallTestAfterSnapshotRecovery).await;
}

#[derive(Debug)]
struct EstimateGasTest {
    gas_limit_threshold: Arc<AtomicU32>,
    snapshot_recovery: bool,
}

impl EstimateGasTest {
    fn new(snapshot_recovery: bool) -> Self {
        Self {
            gas_limit_threshold: Arc::default(),
            snapshot_recovery,
        }
    }
}

#[async_trait]
impl HttpTest for EstimateGasTest {
    fn storage_initialization(&self) -> StorageInitialization {
        let snapshot_recovery = self.snapshot_recovery;
        SendRawTransactionTest { snapshot_recovery }.storage_initialization()
    }

    fn transaction_executor(&self) -> MockTransactionExecutor {
        let mut tx_executor = MockTransactionExecutor::default();
        let pending_block_number = if self.snapshot_recovery {
            StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 2
        } else {
            MiniblockNumber(1)
        };
        let gas_limit_threshold = self.gas_limit_threshold.clone();
        tx_executor.set_call_responses(move |tx, block_args| {
            assert_eq!(tx.execute.calldata(), [] as [u8; 0]);
            assert_eq!(tx.nonce(), Some(Nonce(0)));
            assert_eq!(block_args.resolved_block_number(), pending_block_number);

            let gas_limit_threshold = gas_limit_threshold.load(Ordering::SeqCst);
            if tx.gas_limit() >= U256::from(gas_limit_threshold) {
                ExecutionResult::Success { output: vec![] }
            } else {
                ExecutionResult::Revert {
                    output: VmRevertReason::VmError,
                }
            }
        });
        tx_executor
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let l2_transaction = create_l2_transaction(10, 100);
        for threshold in [10_000, 50_000, 100_000, 1_000_000] {
            self.gas_limit_threshold.store(threshold, Ordering::Relaxed);
            let output = client
                .estimate_gas(l2_transaction.clone().into(), None)
                .await?;
            assert!(
                output >= U256::from(threshold),
                "{output} for threshold {threshold}"
            );
            assert!(
                output < U256::from(threshold) * 2,
                "{output} for threshold {threshold}"
            );
        }

        // Check transaction with value.
        if !self.snapshot_recovery {
            // Manually set sufficient balance for the transaction account.
            let storage_log = SendRawTransactionTest::balance_storage_log();
            let mut storage = pool.connection().await?;
            storage
                .storage_logs_dal()
                .append_storage_logs(MiniblockNumber(0), &[(H256::zero(), vec![storage_log])])
                .await?;
        }
        let mut call_request = CallRequest::from(l2_transaction);
        call_request.from = Some(SendRawTransactionTest::private_key_and_address().1);
        call_request.value = Some(1_000_000.into());
        client.estimate_gas(call_request.clone(), None).await?;

        call_request.value = Some(U256::max_value());
        let error = client.estimate_gas(call_request, None).await.unwrap_err();
        if let ClientError::Call(error) = error {
            let error_msg = error.message();
            assert!(
                error_msg.to_lowercase().contains("insufficient"),
                "{error_msg}"
            );
        } else {
            panic!("Unexpected error: {error:?}");
        }
        Ok(())
    }
}

#[tokio::test]
async fn estimate_gas_basics() {
    test_http_server(EstimateGasTest::new(false)).await;
}

#[tokio::test]
async fn estimate_gas_after_snapshot_recovery() {
    test_http_server(EstimateGasTest::new(true)).await;
}
