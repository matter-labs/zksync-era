//! Tests for the VM-instantiating methods (e.g., `eth_call`).

// TODO: Test other VM methods (`debug_traceCall`, `eth_estimateGas`)

use multivm::interface::ExecutionResult;
use zksync_types::{
    get_intrinsic_constants, transaction_request::CallRequest, L2ChainId, PackedEthSignature, U256,
};
use zksync_utils::u256_to_h256;

use super::*;

#[derive(Debug)]
struct CallTest;

impl CallTest {
    fn call_request() -> CallRequest {
        CallRequest {
            from: Some(Address::repeat_byte(1)),
            to: Some(Address::repeat_byte(2)),
            data: Some(b"call".to_vec().into()),
            ..CallRequest::default()
        }
    }
}

#[async_trait]
impl HttpTest for CallTest {
    fn transaction_executor(&self) -> MockTransactionExecutor {
        let mut tx_executor = MockTransactionExecutor::default();
        tx_executor.insert_call_response(
            Self::call_request().data.unwrap().0,
            ExecutionResult::Success {
                output: b"output".to_vec(),
            },
        );
        tx_executor
    }

    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool) -> anyhow::Result<()> {
        let call_result = client.call(Self::call_request(), None).await?;
        assert_eq!(call_result.0, b"output");

        let valid_block_numbers = [
            api::BlockNumber::Pending,
            api::BlockNumber::Latest,
            0.into(),
        ];
        for number in valid_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number);
            let call_result = client.call(Self::call_request(), Some(number)).await?;
            assert_eq!(call_result.0, b"output");
        }

        let invalid_block_number = api::BlockNumber::from(100);
        let number = api::BlockIdVariant::BlockNumber(invalid_block_number);
        let error = client
            .call(Self::call_request(), Some(number))
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
        CallTest.transaction_executor()
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let call_result = client.call(CallTest::call_request(), None).await?;
        assert_eq!(call_result.0, b"output");
        let pending_block_number = api::BlockIdVariant::BlockNumber(api::BlockNumber::Pending);
        let call_result = client
            .call(CallTest::call_request(), Some(pending_block_number))
            .await?;
        assert_eq!(call_result.0, b"output");

        let first_local_miniblock = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        let first_miniblock_numbers = [api::BlockNumber::Latest, first_local_miniblock.into()];
        for number in first_miniblock_numbers {
            let number = api::BlockIdVariant::BlockNumber(number);
            let error = client
                .call(CallTest::call_request(), Some(number))
                .await
                .unwrap_err();
            if let ClientError::Call(error) = error {
                assert_eq!(error.code(), ErrorCode::InvalidParams.code());
            } else {
                panic!("Unexpected error: {error:?}");
            }
        }

        let pruned_block_numbers = [0, 1, StorageInitialization::SNAPSHOT_RECOVERY_BLOCK];
        for number in pruned_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number.into());
            let error = client
                .call(CallTest::call_request(), Some(number))
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1);
        }

        let mut storage = pool.access_storage().await?;
        store_miniblock(&mut storage, MiniblockNumber(first_local_miniblock), &[]).await?;
        drop(storage);

        for number in first_miniblock_numbers {
            let number = api::BlockIdVariant::BlockNumber(number);
            let call_result = client.call(CallTest::call_request(), Some(number)).await?;
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
        let private_key = H256::repeat_byte(11);
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();

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

    fn balance_storage_log() -> StorageLog {
        let private_key = H256::repeat_byte(11);
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();
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
        tx_executor.insert_tx_response(
            Self::transaction_bytes_and_hash().1,
            ExecutionResult::Success { output: vec![] },
        );
        tx_executor
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        if !self.snapshot_recovery {
            // Manually set sufficient balance for the transaction account.
            let mut storage = pool.access_storage().await?;
            storage
                .storage_dal()
                .apply_storage_logs(&[(H256::zero(), vec![Self::balance_storage_log()])])
                .await;
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
