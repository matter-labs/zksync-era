//! Tests for the VM-instantiating methods (e.g., `eth_call`).

use std::{
    str,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};

use api::state_override::{OverrideAccount, StateOverride};
use test_casing::test_casing;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_multivm::interface::{
    ExecutionResult, OneshotEnv, VmExecutionLogs, VmExecutionResultAndLogs, VmRevertReason,
};
use zksync_types::{
    api::ApiStorageLog, fee_model::BatchFeeInput, get_intrinsic_constants,
    transaction_request::CallRequest, u256_to_h256, K256PrivateKey, L2ChainId, PackedEthSignature,
    StorageLogKind, StorageLogWithPreviousValue, Transaction, U256,
};
use zksync_vm_executor::oneshot::MockOneshotExecutor;
use zksync_web3_decl::namespaces::{DebugNamespaceClient, UnstableNamespaceClient};

use super::*;

#[derive(Debug, Clone)]
struct ExpectedFeeInput(Arc<Mutex<BatchFeeInput>>);

impl Default for ExpectedFeeInput {
    fn default() -> Self {
        let this = Self(Arc::default());
        this.expect_default(1.0); // works for transaction execution and calls
        this
    }
}

impl ExpectedFeeInput {
    fn expect_for_block(&self, number: api::BlockNumber, scale: f64) {
        *self.0.lock().unwrap() = match number {
            api::BlockNumber::Number(number) => create_l2_block(number.as_u32()).batch_fee_input,
            _ => scaled_sensible_fee_input(scale),
        };
    }

    fn expect_default(&self, scale: f64) {
        self.expect_for_block(api::BlockNumber::Pending, scale);
    }

    fn expect_custom(&self, expected: BatchFeeInput) {
        *self.0.lock().unwrap() = expected;
    }

    fn assert_eq(&self, actual: BatchFeeInput) {
        let expected = *self.0.lock().unwrap();
        // We do relaxed comparisons to deal with the fact that the fee input provider may convert inputs to pubdata independent form.
        assert_eq!(
            actual.into_pubdata_independent(),
            expected.into_pubdata_independent()
        );
    }
}

/// Fetches base contract hashes from the genesis block.
async fn genesis_contract_hashes(
    connection: &mut Connection<'_, Core>,
) -> anyhow::Result<BaseSystemContractsHashes> {
    Ok(connection
        .blocks_dal()
        .get_l2_block_header(L2BlockNumber(0))
        .await?
        .context("no genesis block")?
        .base_system_contracts_hashes)
}

#[derive(Debug, Default)]
struct CallTest {
    fee_input: ExpectedFeeInput,
}

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

    fn create_executor(
        latest_block: L2BlockNumber,
        expected_fee_input: ExpectedFeeInput,
    ) -> MockOneshotExecutor {
        let mut tx_executor = MockOneshotExecutor::default();
        tx_executor.set_call_responses(move |tx, env| {
            expected_fee_input.assert_eq(env.l1_batch.fee_input);

            let expected_block_number = match tx.execute.calldata() {
                b"pending" => latest_block.0 + 1,
                b"latest" => latest_block.0,
                block if block.starts_with(b"block=") => str::from_utf8(block)
                    .expect("non-UTF8 calldata")
                    .strip_prefix("block=")
                    .unwrap()
                    .trim()
                    .parse()
                    .expect("invalid block number"),
                data => panic!("Unexpected calldata: {data:?}"),
            };
            assert_eq!(env.l1_batch.first_l2_block.number, expected_block_number);

            ExecutionResult::Success {
                output: b"output".to_vec(),
            }
        });
        tx_executor
    }
}

#[async_trait]
impl HttpTest for CallTest {
    fn transaction_executor(&self) -> MockOneshotExecutor {
        Self::create_executor(L2BlockNumber(1), self.fee_input.clone())
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        // Store an additional L2 block because L2 block #0 has some special processing making it work incorrectly.
        let mut connection = pool.connection().await?;
        store_l2_block(&mut connection, L2BlockNumber(1), &[]).await?;

        let call_result = client
            .call(Self::call_request(b"pending"), None, None)
            .await?;
        assert_eq!(call_result.0, b"output");

        let valid_block_numbers_and_calldata = [
            (api::BlockNumber::Pending, b"pending" as &[_]),
            (api::BlockNumber::Latest, b"latest"),
            (1.into(), b"latest"),
        ];
        for (number, calldata) in valid_block_numbers_and_calldata {
            self.fee_input.expect_for_block(number, 1.0);
            let number = api::BlockIdVariant::BlockNumber(number);
            let call_result = client
                .call(Self::call_request(calldata), Some(number), None)
                .await?;
            assert_eq!(call_result.0, b"output");
        }

        let invalid_block_number = api::BlockNumber::from(100);
        let number = api::BlockIdVariant::BlockNumber(invalid_block_number);
        let error = client
            .call(Self::call_request(b"100"), Some(number), None)
            .await
            .unwrap_err();
        if let ClientError::Call(error) = error {
            assert_eq!(error.code(), ErrorCode::InvalidParams.code());
        } else {
            panic!("Unexpected error: {error:?}");
        }

        // Check that the method handler fetches fee input from the open batch. To do that, we open a new batch
        // with a large fee input; it should be loaded by `ApiFeeInputProvider` and used instead of the input
        // provided by the wrapped mock provider.
        let batch_header = open_l1_batch(
            &mut connection,
            L1BatchNumber(1),
            scaled_sensible_fee_input(3.0),
        )
        .await?;
        // Fee input is not scaled further as per `ApiFeeInputProvider` implementation
        self.fee_input.expect_custom(batch_header.fee_input);
        let call_request = Self::call_request(b"block=2");
        let call_result = client.call(call_request.clone(), None, None).await?;
        assert_eq!(call_result.0, b"output");
        let call_result = client
            .call(
                call_request,
                Some(api::BlockIdVariant::BlockNumber(api::BlockNumber::Pending)),
                None,
            )
            .await?;
        assert_eq!(call_result.0, b"output");

        // Logic here is arguable, but we consider "latest" requests to be interested in the newly
        // open batch's fee input even if the latest block was sealed in the previous batch.
        let call_request = Self::call_request(b"block=1");
        let call_result = client
            .call(
                call_request.clone(),
                Some(api::BlockIdVariant::BlockNumber(api::BlockNumber::Latest)),
                None,
            )
            .await?;
        assert_eq!(call_result.0, b"output");

        let call_request_without_target = CallRequest {
            to: None,
            ..Self::call_request(b"block=2")
        };
        let err = client
            .call(call_request_without_target, None, None)
            .await
            .unwrap_err();
        assert_null_to_address_error(&err);

        Ok(())
    }
}

fn assert_null_to_address_error(error: &ClientError) {
    if let ClientError::Call(error) = error {
        assert_eq!(error.code(), 3);
        assert!(error.message().contains("toAddressIsNull"), "{error:?}");
        assert!(error.data().is_none(), "{error:?}");
    } else {
        panic!("Unexpected error: {error:?}");
    }
}

#[tokio::test]
async fn call_method_basics() {
    test_http_server(CallTest::default()).await;
}

fn evm_emulator_responses(tx: &Transaction, env: &OneshotEnv) -> ExecutionResult {
    assert!(env
        .system
        .base_system_smart_contracts
        .evm_emulator
        .is_some());
    match tx.execute.calldata.as_slice() {
        b"no_target" => assert_eq!(tx.recipient_account(), None),
        _ => assert!(tx.recipient_account().is_some()),
    }
    ExecutionResult::Success {
        output: b"output".to_vec(),
    }
}

#[derive(Debug)]
struct CallTestWithEvmEmulator;

#[async_trait]
impl HttpTest for CallTestWithEvmEmulator {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::genesis_with_evm()
    }

    fn transaction_executor(&self) -> MockOneshotExecutor {
        let mut executor = MockOneshotExecutor::default();
        executor.set_call_responses(evm_emulator_responses);
        executor
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        // Store an additional L2 block because L2 block #0 has some special processing making it work incorrectly.
        let mut connection = pool.connection().await?;
        let block_header = L2BlockHeader {
            base_system_contracts_hashes: genesis_contract_hashes(&mut connection).await?,
            ..create_l2_block(1)
        };
        store_custom_l2_block(&mut connection, &block_header, &[]).await?;

        let call_result = client.call(CallTest::call_request(&[]), None, None).await?;
        assert_eq!(call_result.0, b"output");

        let call_request_without_target = CallRequest {
            to: None,
            ..CallTest::call_request(b"no_target")
        };
        let call_result = client.call(call_request_without_target, None, None).await?;
        assert_eq!(call_result.0, b"output");
        Ok(())
    }
}

#[tokio::test]
async fn call_method_with_evm_emulator() {
    test_http_server(CallTestWithEvmEmulator).await;
}

#[derive(Debug, Default)]
struct CallTestAfterSnapshotRecovery {
    fee_input: ExpectedFeeInput,
}

#[async_trait]
impl HttpTest for CallTestAfterSnapshotRecovery {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::empty_recovery()
    }

    fn transaction_executor(&self) -> MockOneshotExecutor {
        let first_local_l2_block = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        CallTest::create_executor(first_local_l2_block, self.fee_input.clone())
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        _pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let call_result = client
            .call(CallTest::call_request(b"pending"), None, None)
            .await?;
        assert_eq!(call_result.0, b"output");
        let pending_block_number = api::BlockIdVariant::BlockNumber(api::BlockNumber::Pending);
        let call_result = client
            .call(
                CallTest::call_request(b"pending"),
                Some(pending_block_number),
                None,
            )
            .await?;
        assert_eq!(call_result.0, b"output");

        let first_local_l2_block = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        let pruned_block_numbers = [0, 1, StorageInitialization::SNAPSHOT_RECOVERY_BLOCK.0];
        for number in pruned_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number.into());
            let error = client
                .call(CallTest::call_request(b"pruned"), Some(number), None)
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, first_local_l2_block);
        }

        let first_l2_block_numbers = [api::BlockNumber::Latest, first_local_l2_block.0.into()];
        for number in first_l2_block_numbers {
            self.fee_input.expect_for_block(number, 1.0);
            let number = api::BlockIdVariant::BlockNumber(number);
            let call_result = client
                .call(CallTest::call_request(b"latest"), Some(number), None)
                .await?;
            assert_eq!(call_result.0, b"output");
        }
        Ok(())
    }
}

#[tokio::test]
async fn call_method_after_snapshot_recovery() {
    test_http_server(CallTestAfterSnapshotRecovery::default()).await;
}

#[derive(Debug)]
struct CallTestWithSlowVm;

#[async_trait]
impl HttpTest for CallTestWithSlowVm {
    fn transaction_executor(&self) -> MockOneshotExecutor {
        let mut tx_executor = MockOneshotExecutor::default();
        tx_executor.set_vm_delay(Duration::from_secs(3_600));
        tx_executor.set_call_responses(|_, _| ExecutionResult::Success { output: vec![] });
        tx_executor
    }

    fn web3_config(&self) -> Web3JsonRpcConfig {
        Web3JsonRpcConfig {
            request_timeout: Some(Duration::from_secs(3)),
            ..Web3JsonRpcConfig::for_tests()
        }
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        _pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let err = client
            .call(CallTest::call_request(b"pending"), None, None)
            .await
            .unwrap_err();
        if let ClientError::Call(error) = err {
            assert_eq!(error.code(), 503);
            assert!(error.message().contains("timed out"), "{error:?}");
        } else {
            panic!("Unexpected error: {err:?}");
        }
        Ok(())
    }
}

#[tokio::test]
async fn call_method_with_slow_vm() {
    test_http_server(CallTestWithSlowVm).await;
}

#[derive(Debug)]
struct SendRawTransactionTest {
    snapshot_recovery: bool,
}

impl SendRawTransactionTest {
    fn transaction_bytes_and_hash(include_to: bool) -> (Vec<u8>, H256) {
        let private_key = Self::private_key();
        let tx_request = api::TransactionRequest {
            chain_id: Some(L2ChainId::default().as_u64()),
            from: Some(private_key.address()),
            to: include_to.then(|| Address::repeat_byte(2)),
            value: 123_456.into(),
            gas: (get_intrinsic_constants().l2_tx_intrinsic_gas * 2).into(),
            gas_price: StateKeeperConfig::for_tests().minimal_l2_gas_price.into(),
            input: if include_to {
                vec![1, 2, 3, 4].into()
            } else {
                b"no_target".to_vec().into()
            },
            ..api::TransactionRequest::default()
        };
        let data = tx_request.get_rlp().unwrap();
        let signed_message = PackedEthSignature::message_to_signed_bytes(&data);
        let signature = PackedEthSignature::sign_raw(&private_key, &signed_message).unwrap();

        let mut rlp = Default::default();
        tx_request.rlp(&mut rlp, Some(&signature)).unwrap();
        let data = rlp.out();
        let (_, tx_hash) =
            api::TransactionRequest::from_bytes(&data, L2ChainId::default()).unwrap();
        (data.into(), tx_hash)
    }

    fn private_key() -> K256PrivateKey {
        K256PrivateKey::from_bytes(H256::repeat_byte(11)).unwrap()
    }

    fn balance_storage_log() -> StorageLog {
        let balance_key = storage_key_for_eth_balance(&Self::private_key().address());
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
            StorageInitialization::genesis()
        }
    }

    fn transaction_executor(&self) -> MockOneshotExecutor {
        let mut tx_executor = MockOneshotExecutor::default();
        let pending_block = if self.snapshot_recovery {
            StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 2
        } else {
            L2BlockNumber(1)
        };
        tx_executor.set_tx_responses(move |tx, env| {
            assert_eq!(tx.hash(), Self::transaction_bytes_and_hash(true).1);
            assert_eq!(env.l1_batch.first_l2_block.number, pending_block.0);
            ExecutionResult::Success { output: vec![] }
        });
        tx_executor
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        if !self.snapshot_recovery {
            // Manually set sufficient balance for the transaction account.
            let mut storage = pool.connection().await?;
            storage
                .storage_logs_dal()
                .append_storage_logs(L2BlockNumber(0), &[Self::balance_storage_log()])
                .await?;
        }

        let (tx_bytes, tx_hash) = Self::transaction_bytes_and_hash(true);
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
struct SendRawTransactionWithoutToAddressTest;

#[async_trait]
impl HttpTest for SendRawTransactionWithoutToAddressTest {
    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let mut storage = pool.connection().await?;
        storage
            .storage_logs_dal()
            .append_storage_logs(
                L2BlockNumber(0),
                &[SendRawTransactionTest::balance_storage_log()],
            )
            .await?;

        let (tx_bytes, _) = SendRawTransactionTest::transaction_bytes_and_hash(false);
        let err = client
            .send_raw_transaction(tx_bytes.into())
            .await
            .unwrap_err();
        assert_null_to_address_error(&err);
        Ok(())
    }
}

#[tokio::test]
async fn send_raw_transaction_fails_without_to_address() {
    test_http_server(SendRawTransactionWithoutToAddressTest).await;
}

#[derive(Debug)]
struct SendRawTransactionTestWithEvmEmulator;

#[async_trait]
impl HttpTest for SendRawTransactionTestWithEvmEmulator {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::genesis_with_evm()
    }

    fn transaction_executor(&self) -> MockOneshotExecutor {
        let mut executor = MockOneshotExecutor::default();
        executor.set_tx_responses(evm_emulator_responses);
        executor
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        // Manually set sufficient balance for the transaction account.
        let mut storage = pool.connection().await?;
        storage
            .storage_logs_dal()
            .append_storage_logs(
                L2BlockNumber(0),
                &[SendRawTransactionTest::balance_storage_log()],
            )
            .await?;

        let (tx_bytes, tx_hash) = SendRawTransactionTest::transaction_bytes_and_hash(true);
        let send_result = client.send_raw_transaction(tx_bytes.into()).await?;
        assert_eq!(send_result, tx_hash);

        let (tx_bytes, tx_hash) = SendRawTransactionTest::transaction_bytes_and_hash(false);
        let send_result = client.send_raw_transaction(tx_bytes.into()).await?;
        assert_eq!(send_result, tx_hash);
        Ok(())
    }
}

#[tokio::test]
async fn send_raw_transaction_with_evm_emulator() {
    test_http_server(SendRawTransactionTestWithEvmEmulator).await;
}

#[derive(Debug)]
struct SendTransactionWithDetailedOutputTest;

impl SendTransactionWithDetailedOutputTest {
    fn storage_logs(&self) -> Vec<StorageLogWithPreviousValue> {
        let log = StorageLog {
            key: StorageKey::new(
                AccountTreeId::new(Address::zero()),
                u256_to_h256(U256::one()),
            ),
            value: u256_to_h256(U256::one()),
            kind: StorageLogKind::Read,
        };
        [
            StorageLog {
                kind: StorageLogKind::Read,
                ..log
            },
            StorageLog {
                kind: StorageLogKind::InitialWrite,
                ..log
            },
            StorageLog {
                kind: StorageLogKind::RepeatedWrite,
                ..log
            },
        ]
        .into_iter()
        .map(|log| StorageLogWithPreviousValue {
            log,
            previous_value: u256_to_h256(U256::one()),
        })
        .collect()
    }

    fn vm_events(&self) -> Vec<VmEvent> {
        vec![VmEvent {
            location: (L1BatchNumber(1), 1),
            address: Address::zero(),
            indexed_topics: Vec::new(),
            value: Vec::new(),
        }]
    }
}
#[async_trait]
impl HttpTest for SendTransactionWithDetailedOutputTest {
    fn transaction_executor(&self) -> MockOneshotExecutor {
        let mut tx_executor = MockOneshotExecutor::default();
        let tx_bytes_and_hash = SendRawTransactionTest::transaction_bytes_and_hash(true);
        let vm_execution_logs = VmExecutionLogs {
            storage_logs: self.storage_logs(),
            events: self.vm_events(),
            user_l2_to_l1_logs: Default::default(),
            system_l2_to_l1_logs: Default::default(),
            total_log_queries_count: 0,
        };

        tx_executor.set_full_tx_responses(move |tx, env| {
            assert_eq!(tx.hash(), tx_bytes_and_hash.1);
            assert_eq!(env.l1_batch.first_l2_block.number, 1);

            VmExecutionResultAndLogs {
                logs: vm_execution_logs.clone(),
                ..VmExecutionResultAndLogs::mock_success()
            }
        });
        tx_executor
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        // Manually set sufficient balance for the transaction account.
        let mut storage = pool.connection().await?;
        storage
            .storage_logs_dal()
            .append_storage_logs(
                L2BlockNumber(0),
                &[SendRawTransactionTest::balance_storage_log()],
            )
            .await?;

        let (tx_bytes, tx_hash) = SendRawTransactionTest::transaction_bytes_and_hash(true);
        let send_result = client
            .send_raw_transaction_with_detailed_output(tx_bytes.into())
            .await?;
        assert_eq!(send_result.transaction_hash, tx_hash);

        let expected_events = self.vm_events();
        assert_eq!(send_result.events.len(), expected_events.len());
        for (event, expected_event) in send_result.events.iter().zip(&expected_events) {
            assert_eq!(event.transaction_hash, Some(tx_hash));
            assert_eq!(event.address, expected_event.address);
            assert_eq!(event.topics, expected_event.indexed_topics);
            assert_eq!(event.l1_batch_number, Some(1.into()));
            assert_eq!(event.transaction_index, Some(1.into()));
        }

        assert_eq!(
            send_result.storage_logs,
            self.storage_logs()
                .iter()
                .filter(|x| x.log.is_write())
                .map(ApiStorageLog::from)
                .collect::<Vec<_>>()
        );
        Ok(())
    }
}

#[tokio::test]
async fn send_raw_transaction_with_detailed_output() {
    test_http_server(SendTransactionWithDetailedOutputTest).await;
}

#[derive(Debug, Default)]
struct TraceCallTest {
    fee_input: ExpectedFeeInput,
}

impl TraceCallTest {
    const FEE_SCALE: f64 = 1.2; // set in the tx sender config

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
    fn transaction_executor(&self) -> MockOneshotExecutor {
        CallTest::create_executor(L2BlockNumber(1), self.fee_input.clone())
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        // Store an additional L2 block because L2 block #0 has some special processing making it work incorrectly.
        // First half of the test asserts API server's behavior when there is no open batch. In other words,
        // when `ApiFeeInputProvider` is forced to fetch fee params from the main fee provider.
        let mut connection = pool.connection().await?;
        store_l2_block(&mut connection, L2BlockNumber(1), &[]).await?;
        seal_l1_batch(&mut connection, L1BatchNumber(1)).await?;

        self.fee_input.expect_default(Self::FEE_SCALE);
        let call_request = CallTest::call_request(b"pending");
        let call_result = client
            .trace_call(call_request.clone(), None, None)
            .await?
            .unwrap_default();
        Self::assert_debug_call(&call_request, &call_result);
        let pending_block_number = api::BlockId::Number(api::BlockNumber::Pending);
        let call_result = client
            .trace_call(call_request.clone(), Some(pending_block_number), None)
            .await?
            .unwrap_default();
        Self::assert_debug_call(&call_request, &call_result);

        let latest_block_numbers = [api::BlockNumber::Latest, 1.into()];
        let call_request = CallTest::call_request(b"latest");
        for number in latest_block_numbers {
            self.fee_input.expect_for_block(number, Self::FEE_SCALE);
            let call_result = client
                .trace_call(
                    call_request.clone(),
                    Some(api::BlockId::Number(number)),
                    None,
                )
                .await?
                .unwrap_default();
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

        // Check that the method handler fetches fee input from the open batch. To do that, we open a new batch
        // with a large fee input; it should be loaded by `ApiFeeInputProvider` and used instead of the input
        // provided by the wrapped mock provider.
        let batch_header = open_l1_batch(
            &mut connection,
            L1BatchNumber(2),
            scaled_sensible_fee_input(3.0),
        )
        .await?;
        // Fee input is not scaled further as per `ApiFeeInputProvider` implementation
        self.fee_input.expect_custom(
            batch_header
                .fee_input
                .scale_linearly(Self::FEE_SCALE, Self::FEE_SCALE),
        );
        let call_request = CallTest::call_request(b"block=2");
        let call_result = client.trace_call(call_request.clone(), None, None).await?;
        Self::assert_debug_call(&call_request, &call_result.unwrap_default());
        let call_result = client
            .trace_call(
                call_request.clone(),
                Some(api::BlockId::Number(api::BlockNumber::Pending)),
                None,
            )
            .await?;
        Self::assert_debug_call(&call_request, &call_result.unwrap_default());

        // Logic here is arguable, but we consider "latest" requests to be interested in the newly
        // open batch's fee input even if the latest block was sealed in the previous batch.
        let call_request = CallTest::call_request(b"block=1");
        let call_result = client
            .trace_call(
                call_request.clone(),
                Some(api::BlockId::Number(api::BlockNumber::Latest)),
                None,
            )
            .await?;
        Self::assert_debug_call(&call_request, &call_result.unwrap_default());

        let call_request_without_target = CallRequest {
            to: None,
            ..CallTest::call_request(b"block=2")
        };
        let err = client
            .call(call_request_without_target, None, None)
            .await
            .unwrap_err();
        assert_null_to_address_error(&err);

        Ok(())
    }
}

#[tokio::test]
async fn trace_call_basics() {
    test_http_server(TraceCallTest::default()).await;
}

#[derive(Debug, Default)]
struct TraceCallTestAfterSnapshotRecovery {
    fee_input: ExpectedFeeInput,
}

#[async_trait]
impl HttpTest for TraceCallTestAfterSnapshotRecovery {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::empty_recovery()
    }

    fn transaction_executor(&self) -> MockOneshotExecutor {
        let number = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        CallTest::create_executor(number, self.fee_input.clone())
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        _pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        self.fee_input.expect_default(TraceCallTest::FEE_SCALE);
        let call_request = CallTest::call_request(b"pending");
        let call_result = client
            .trace_call(call_request.clone(), None, None)
            .await?
            .unwrap_default();
        TraceCallTest::assert_debug_call(&call_request, &call_result);
        let pending_block_number = api::BlockId::Number(api::BlockNumber::Pending);
        let call_result = client
            .trace_call(call_request.clone(), Some(pending_block_number), None)
            .await?
            .unwrap_default();
        TraceCallTest::assert_debug_call(&call_request, &call_result);

        let first_local_l2_block = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        let pruned_block_numbers = [0, 1, StorageInitialization::SNAPSHOT_RECOVERY_BLOCK.0];
        for number in pruned_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number.into());
            let error = client
                .call(CallTest::call_request(b"pruned"), Some(number), None)
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, first_local_l2_block);
        }

        let call_request = CallTest::call_request(b"latest");
        let first_l2_block_numbers = [api::BlockNumber::Latest, first_local_l2_block.0.into()];
        for number in first_l2_block_numbers {
            self.fee_input
                .expect_for_block(number, TraceCallTest::FEE_SCALE);
            let number = api::BlockId::Number(number);
            let call_result = client
                .trace_call(call_request.clone(), Some(number), None)
                .await?
                .unwrap_default();
            TraceCallTest::assert_debug_call(&call_request, &call_result);
        }
        Ok(())
    }
}

#[tokio::test]
async fn trace_call_after_snapshot_recovery() {
    test_http_server(TraceCallTestAfterSnapshotRecovery::default()).await;
}

#[derive(Debug)]
struct TraceCallTestWithEvmEmulator;

#[async_trait]
impl HttpTest for TraceCallTestWithEvmEmulator {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::genesis_with_evm()
    }

    fn transaction_executor(&self) -> MockOneshotExecutor {
        let mut executor = MockOneshotExecutor::default();
        executor.set_call_responses(evm_emulator_responses);
        executor
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        // Store an additional L2 block because L2 block #0 has some special processing making it work incorrectly.
        // And make sure there is no open batch so that `ApiFeeInputProvider` is forced to fetch fee params from
        // the main fee provider.
        let mut connection = pool.connection().await?;
        let block_header = L2BlockHeader {
            base_system_contracts_hashes: genesis_contract_hashes(&mut connection).await?,
            ..create_l2_block(1)
        };
        store_custom_l2_block(&mut connection, &block_header, &[]).await?;
        seal_l1_batch(&mut connection, L1BatchNumber(1)).await?;

        client
            .trace_call(CallTest::call_request(&[]), None, None)
            .await?;

        let call_request_without_target = CallRequest {
            to: None,
            ..CallTest::call_request(b"no_target")
        };
        client
            .trace_call(call_request_without_target, None, None)
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn trace_call_method_with_evm_emulator() {
    test_http_server(TraceCallTestWithEvmEmulator).await;
}

#[derive(Debug, Clone, Copy)]
enum EstimateMethod {
    EthEstimateGas,
    ZksEstimateFee,
    ZksEstimateGasL1ToL2,
}

impl EstimateMethod {
    const ALL: [Self; 3] = [
        Self::EthEstimateGas,
        Self::ZksEstimateFee,
        Self::ZksEstimateGasL1ToL2,
    ];

    async fn query(self, client: &DynClient<L2>, req: CallRequest) -> Result<U256, ClientError> {
        match self {
            Self::EthEstimateGas => client.estimate_gas(req, None, None).await,
            Self::ZksEstimateFee => client
                .estimate_fee(req, None)
                .await
                .map(|fee| fee.gas_limit),
            Self::ZksEstimateGasL1ToL2 => client.estimate_gas_l1_to_l2(req, None).await,
        }
    }
}

#[derive(Debug)]
struct EstimateGasTest {
    gas_limit_threshold: Arc<AtomicU32>,
    method: EstimateMethod,
    snapshot_recovery: bool,
}

impl EstimateGasTest {
    fn new(method: EstimateMethod, snapshot_recovery: bool) -> Self {
        Self {
            gas_limit_threshold: Arc::default(),
            method,
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

    fn transaction_executor(&self) -> MockOneshotExecutor {
        let mut tx_executor = MockOneshotExecutor::default();
        let pending_block_number = if self.snapshot_recovery {
            StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 2
        } else {
            L2BlockNumber(1)
        };
        let gas_limit_threshold = self.gas_limit_threshold.clone();
        let should_set_nonce = !matches!(self.method, EstimateMethod::ZksEstimateGasL1ToL2);
        tx_executor.set_tx_responses(move |tx, env| {
            assert_eq!(tx.execute.calldata(), [] as [u8; 0]);
            if should_set_nonce {
                assert_eq!(tx.nonce(), Some(Nonce(0)));
            }
            assert_eq!(env.l1_batch.first_l2_block.number, pending_block_number.0);

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

    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let l2_transaction = create_l2_transaction(10, 100);
        for threshold in [10_000, 50_000, 100_000, 1_000_000] {
            self.gas_limit_threshold.store(threshold, Ordering::Relaxed);
            let output = self
                .method
                .query(client, l2_transaction.clone().into())
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
                .append_storage_logs(L2BlockNumber(0), &[storage_log])
                .await?;
        }
        let mut call_request = CallRequest::from(l2_transaction);
        call_request.from = Some(SendRawTransactionTest::private_key().address());
        call_request.value = Some(1_000_000.into());

        self.method.query(client, call_request.clone()).await?;

        call_request.value = Some(U256::max_value());
        let error = self.method.query(client, call_request).await.unwrap_err();
        if let ClientError::Call(error) = error {
            let error_msg = error.message();
            // L1 and L2 transactions have differing error messages in this case.
            assert!(
                error_msg.to_lowercase().contains("insufficient")
                    || error_msg.to_lowercase().contains("overflow"),
                "{error_msg}"
            );
        } else {
            panic!("Unexpected error: {error:?}");
        }
        Ok(())
    }
}

#[test_casing(3, EstimateMethod::ALL)]
#[tokio::test]
async fn estimate_gas_basics(method: EstimateMethod) {
    test_http_server(EstimateGasTest::new(method, false)).await;
}

#[test_casing(3, EstimateMethod::ALL)]
#[tokio::test]
async fn estimate_gas_after_snapshot_recovery(method: EstimateMethod) {
    test_http_server(EstimateGasTest::new(method, true)).await;
}

#[derive(Debug)]
struct EstimateGasWithStateOverrideTest {
    inner: EstimateGasTest,
}

#[async_trait]
impl HttpTest for EstimateGasWithStateOverrideTest {
    fn storage_initialization(&self) -> StorageInitialization {
        self.inner.storage_initialization()
    }

    fn transaction_executor(&self) -> MockOneshotExecutor {
        self.inner.transaction_executor()
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        _pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        // Transaction with balance override
        let l2_transaction = create_l2_transaction(10, 100);
        let mut call_request = CallRequest::from(l2_transaction);
        let request_initiator = Address::random();
        call_request.from = Some(request_initiator);
        call_request.value = Some(1_000_000.into());

        let state_override = HashMap::from([(
            request_initiator,
            OverrideAccount {
                balance: Some(U256::max_value()),
                ..OverrideAccount::default()
            },
        )]);
        let state_override = StateOverride::new(state_override);

        client
            .estimate_gas(call_request.clone(), None, Some(state_override))
            .await?;

        // Transaction that should fail without balance override
        let l2_transaction = create_l2_transaction(10, 100);
        let mut call_request = CallRequest::from(l2_transaction);
        call_request.from = Some(Address::random());
        call_request.value = Some(1_000_000.into());

        let error = client
            .estimate_gas(call_request.clone(), None, None)
            .await
            .unwrap_err();

        if let ClientError::Call(error) = error {
            let error_msg = error.message();
            assert!(
                error_msg.to_lowercase().contains("insufficient funds"),
                "{error_msg}"
            );
        } else {
            panic!("Unexpected error: {error:?}");
        }
        Ok(())
    }
}

#[tokio::test]
async fn estimate_gas_with_state_override() {
    let inner = EstimateGasTest::new(EstimateMethod::EthEstimateGas, false);
    test_http_server(EstimateGasWithStateOverrideTest { inner }).await;
}

#[derive(Debug)]
struct EstimateGasWithoutToAddressTest {
    method: EstimateMethod,
}

#[async_trait]
impl HttpTest for EstimateGasWithoutToAddressTest {
    async fn test(
        &self,
        client: &DynClient<L2>,
        _pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let mut l2_transaction = create_l2_transaction(10, 100);
        l2_transaction.execute.contract_address = None;
        l2_transaction.common_data.signature = vec![]; // Remove invalidated signature so that it doesn't trip estimation logic
        let err = self
            .method
            .query(client, l2_transaction.into())
            .await
            .unwrap_err();
        assert_null_to_address_error(&err);
        Ok(())
    }
}

#[test_casing(3, EstimateMethod::ALL)]
#[tokio::test]
async fn estimate_gas_fails_without_to_address(method: EstimateMethod) {
    test_http_server(EstimateGasWithoutToAddressTest { method }).await;
}

#[derive(Debug)]
struct EstimateGasTestWithEvmEmulator {
    method: EstimateMethod,
}

#[async_trait]
impl HttpTest for EstimateGasTestWithEvmEmulator {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::genesis_with_evm()
    }

    fn transaction_executor(&self) -> MockOneshotExecutor {
        let mut executor = MockOneshotExecutor::default();
        executor.set_tx_responses(evm_emulator_responses);
        executor
    }

    async fn test(
        &self,
        client: &DynClient<L2>,
        _pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let call_request = CallRequest {
            from: Some(Address::repeat_byte(1)),
            to: Some(Address::repeat_byte(2)),
            ..CallRequest::default()
        };
        self.method.query(client, call_request).await?;

        let call_request = CallRequest {
            from: Some(Address::repeat_byte(1)),
            to: None,
            data: Some(b"no_target".to_vec().into()),
            ..CallRequest::default()
        };
        self.method.query(client, call_request).await?;
        Ok(())
    }
}

#[test_casing(3, EstimateMethod::ALL)]
#[tokio::test]
async fn estimate_gas_with_evm_emulator(method: EstimateMethod) {
    test_http_server(EstimateGasTestWithEvmEmulator { method }).await;
}
