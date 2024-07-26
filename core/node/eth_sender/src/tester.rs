use std::sync::Arc;

use zksync_config::{
    configs::eth_sender::{ProofSendingMode, PubdataSendingMode, SenderConfig},
    ContractsConfig, EthConfig, GasAdjusterConfig,
};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::{clients::MockEthereum, BaseFees};
use zksync_l1_contract_interface::i_executor::methods::{ExecuteBatches, ProveBatches};
use zksync_node_fee_model::l1_gas_price::GasAdjuster;
use zksync_node_test_utils::{create_l1_batch, l1_batch_metadata_to_commitment_artifacts};
use zksync_object_store::MockObjectStore;
use zksync_types::{
    block::L1BatchHeader, commitment::L1BatchCommitmentMode, pubdata_da::PubdataDA, Address,
    L1BatchNumber, ProtocolVersion, H256,
};

use crate::{
    abstract_l1_interface::{L1BlockNumbers, OperatorType},
    aggregated_operations::AggregatedOperation,
    tests::{default_l1_batch_metadata, l1_batch_with_metadata},
    Aggregator, EthTxAggregator, EthTxManager,
};

// Alias to conveniently call static methods of `ETHSender`.
type MockEthTxManager = EthTxManager;

pub(crate) struct TestL1Batch {
    l1_batch_number: L1BatchNumber,
}

impl TestL1Batch {
    pub async fn commit(&self, tester: &mut EthSenderTester, confirm: bool) -> H256 {
        assert_ne!(
            self.l1_batch_number,
            L1BatchNumber(0),
            "Cannot commit genesis batch"
        );
        tester.commit_l1_batch(self.l1_batch_number, confirm).await
    }

    pub async fn prove(&self, tester: &mut EthSenderTester, confirm: bool) -> H256 {
        assert_ne!(
            self.l1_batch_number,
            L1BatchNumber(0),
            "Cannot prove genesis batch"
        );
        tester.prove_l1_batch(confirm).await
    }

    pub async fn execute(&self, tester: &mut EthSenderTester, confirm: bool) -> H256 {
        assert_ne!(
            self.l1_batch_number,
            L1BatchNumber(0),
            "Cannot execute genesis batch"
        );
        tester.execute_l1_batch(confirm).await
    }

    pub async fn sealed(tester: &mut EthSenderTester) -> Self {
        tester.seal_l1_batch().await;
        Self {
            l1_batch_number: tester.next_l1_batch_number_to_seal - 1,
        }
    }
}

#[derive(Debug)]
pub(crate) struct EthSenderTester {
    pub conn: ConnectionPool<Core>,
    pub gateway: Box<MockEthereum>,
    pub manager: MockEthTxManager,
    pub aggregator: EthTxAggregator,
    pub gas_adjuster: Arc<GasAdjuster>,
    pub pubdata_sending_mode: PubdataSendingMode,
    next_l1_batch_number_to_seal: L1BatchNumber,
    next_l1_batch_number_to_commit: L1BatchNumber,
    next_l1_batch_number_to_prove: L1BatchNumber,
    next_l1_batch_number_to_execute: L1BatchNumber,
}

impl EthSenderTester {
    pub const WAIT_CONFIRMATIONS: u64 = 10;
    pub const MAX_BASE_FEE_SAMPLES: usize = 3;

    pub async fn new(
        connection_pool: ConnectionPool<Core>,
        history: Vec<u64>,
        non_ordering_confirmations: bool,
        aggregator_operate_4844_mode: bool,
        commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        let eth_sender_config = EthConfig::for_tests();
        let contracts_config = ContractsConfig::for_tests();
        let pubdata_sending_mode = if aggregator_operate_4844_mode {
            PubdataSendingMode::Blobs
        } else {
            PubdataSendingMode::Calldata
        };
        let aggregator_config = SenderConfig {
            aggregated_proof_sizes: vec![1],
            pubdata_sending_mode,
            ..eth_sender_config.clone().sender.unwrap()
        };

        let history: Vec<_> = history
            .into_iter()
            .map(|base_fee_per_gas| BaseFees {
                base_fee_per_gas,
                base_fee_per_blob_gas: 0.into(),
            })
            .collect();

        let gateway = MockEthereum::builder()
            .with_fee_history(
                std::iter::repeat_with(|| BaseFees {
                    base_fee_per_gas: 0,
                    base_fee_per_blob_gas: 0.into(),
                })
                .take(Self::WAIT_CONFIRMATIONS as usize)
                .chain(history)
                .collect(),
            )
            .with_non_ordering_confirmation(non_ordering_confirmations)
            .with_call_handler(move |call, _| {
                assert_eq!(call.to, Some(contracts_config.l1_multicall3_addr));
                crate::tests::mock_multicall_response()
            })
            .build();
        gateway.advance_block_number(Self::WAIT_CONFIRMATIONS);
        let gateway = Box::new(gateway);

        let gas_adjuster = Arc::new(
            GasAdjuster::new(
                Box::new(gateway.clone().into_client()),
                GasAdjusterConfig {
                    max_base_fee_samples: Self::MAX_BASE_FEE_SAMPLES,
                    pricing_formula_parameter_a: 3.0,
                    pricing_formula_parameter_b: 2.0,
                    ..eth_sender_config.gas_adjuster.unwrap()
                },
                pubdata_sending_mode,
                commitment_mode,
            )
            .await
            .unwrap(),
        );

        let eth_sender = eth_sender_config.sender.clone().unwrap();
        let aggregator = EthTxAggregator::new(
            connection_pool.clone(),
            SenderConfig {
                proof_sending_mode: ProofSendingMode::SkipEveryProof,
                pubdata_sending_mode,
                ..eth_sender.clone()
            },
            // Aggregator - unused
            Aggregator::new(
                aggregator_config.clone(),
                MockObjectStore::arc(),
                aggregator_operate_4844_mode,
                commitment_mode,
            ),
            gateway.clone(),
            // ZKsync contract address
            Address::random(),
            contracts_config.l1_multicall3_addr,
            Address::random(),
            Default::default(),
            None,
        )
        .await;

        let manager = EthTxManager::new(
            connection_pool.clone(),
            eth_sender.clone(),
            gas_adjuster.clone(),
            gateway.clone(),
            None,
        );

        let connection_pool_clone = connection_pool.clone();
        let mut storage = connection_pool_clone.connection().await.unwrap();
        storage
            .protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        Self {
            gateway,
            manager,
            aggregator,
            gas_adjuster,
            conn: connection_pool,
            pubdata_sending_mode,
            next_l1_batch_number_to_seal: L1BatchNumber(0),
            next_l1_batch_number_to_commit: L1BatchNumber(1),
            next_l1_batch_number_to_execute: L1BatchNumber(1),
            next_l1_batch_number_to_prove: L1BatchNumber(1),
        }
    }

    pub async fn storage(&self) -> Connection<'_, Core> {
        self.conn.connection().await.unwrap()
    }

    pub async fn get_block_numbers(&self) -> L1BlockNumbers {
        let latest = self
            .manager
            .l1_interface()
            .get_l1_block_numbers()
            .await
            .unwrap()
            .latest;
        let finalized = latest - Self::WAIT_CONFIRMATIONS as u32;
        L1BlockNumbers {
            finalized,
            latest,
            safe: finalized,
        }
    }
    async fn insert_l1_batch(&self, number: L1BatchNumber) -> L1BatchHeader {
        let header = create_l1_batch(number.0);

        // Save L1 batch to the database
        self.storage()
            .await
            .blocks_dal()
            .insert_mock_l1_batch(&header)
            .await
            .unwrap();
        let metadata = default_l1_batch_metadata();
        self.storage()
            .await
            .blocks_dal()
            .save_l1_batch_tree_data(header.number, &metadata.tree_data())
            .await
            .unwrap();
        self.storage()
            .await
            .blocks_dal()
            .save_l1_batch_commitment_artifacts(
                header.number,
                &l1_batch_metadata_to_commitment_artifacts(&metadata),
            )
            .await
            .unwrap();
        header
    }
    pub async fn seal_l1_batch(&mut self) -> L1BatchHeader {
        let header = self
            .insert_l1_batch(self.next_l1_batch_number_to_seal)
            .await;
        self.next_l1_batch_number_to_seal += 1;
        header
    }

    pub async fn execute_l1_batch(&mut self, confirm: bool) -> H256 {
        let operation = AggregatedOperation::Execute(ExecuteBatches {
            l1_batches: vec![
                self.get_l1_batch_header_from_db(self.next_l1_batch_number_to_execute)
                    .await,
            ]
            .into_iter()
            .map(l1_batch_with_metadata)
            .collect(),
        });
        self.next_l1_batch_number_to_execute += 1;
        self.send_operation(operation, confirm).await
    }

    pub async fn prove_l1_batch(&mut self, confirm: bool) -> H256 {
        let operation = AggregatedOperation::PublishProofOnchain(ProveBatches {
            prev_l1_batch: l1_batch_with_metadata(
                self.get_l1_batch_header_from_db(self.next_l1_batch_number_to_prove - 1)
                    .await,
            ),
            l1_batches: vec![l1_batch_with_metadata(
                self.get_l1_batch_header_from_db(self.next_l1_batch_number_to_prove)
                    .await,
            )],
            proofs: vec![],
            should_verify: false,
        });
        self.next_l1_batch_number_to_prove += 1;
        self.send_operation(operation, confirm).await
    }

    pub async fn run_eth_sender_tx_manager_iteration(&mut self) {
        self.manager
            .loop_iteration(
                &mut self.conn.connection().await.unwrap(),
                self.get_block_numbers().await,
            )
            .await;
    }

    async fn get_l1_batch_header_from_db(&mut self, number: L1BatchNumber) -> L1BatchHeader {
        self.conn
            .connection()
            .await
            .unwrap()
            .blocks_dal()
            .get_l1_batch_header(number)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("expected to find header for {}", number))
    }
    pub async fn commit_l1_batch(&mut self, l1_batch_number: L1BatchNumber, confirm: bool) -> H256 {
        assert_eq!(l1_batch_number, self.next_l1_batch_number_to_commit);
        let pubdata_mode = if self.pubdata_sending_mode == PubdataSendingMode::Blobs {
            PubdataDA::Blobs
        } else {
            PubdataDA::Calldata
        };
        let operation = AggregatedOperation::Commit(
            l1_batch_with_metadata(
                self.get_l1_batch_header_from_db(self.next_l1_batch_number_to_commit - 1)
                    .await,
            ),
            vec![l1_batch_with_metadata(
                self.get_l1_batch_header_from_db(self.next_l1_batch_number_to_commit)
                    .await,
            )],
            pubdata_mode,
        );
        self.next_l1_batch_number_to_commit += 1;
        self.send_operation(operation, confirm).await
    }

    pub async fn send_operation(
        &mut self,
        aggregated_operation: AggregatedOperation,
        confirm: bool,
    ) -> H256 {
        let tx = self
            .aggregator
            .save_eth_tx(
                &mut self.conn.connection().await.unwrap(),
                &aggregated_operation,
                false,
            )
            .await
            .unwrap();

        let hash = self
            .manager
            .send_eth_tx(
                &mut self.conn.connection().await.unwrap(),
                &tx,
                0,
                self.get_block_numbers().await.latest,
            )
            .await
            .unwrap();

        if confirm {
            self.confirm_tx(hash).await;
        }
        hash
    }

    pub async fn confirm_tx(&mut self, hash: H256) {
        self.gateway
            .execute_tx(hash, true, EthSenderTester::WAIT_CONFIRMATIONS);
        self.manager
            .monitor_inflight_transactions_single_operator(
                &mut self.conn.connection().await.unwrap(),
                self.get_block_numbers().await,
                OperatorType::NonBlob,
            )
            .await
            .unwrap();

        self.manager
            .monitor_inflight_transactions_single_operator(
                &mut self.conn.connection().await.unwrap(),
                self.get_block_numbers().await,
                OperatorType::Blob,
            )
            .await
            .unwrap();
    }
}
