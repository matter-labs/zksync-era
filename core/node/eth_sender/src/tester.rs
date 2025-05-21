use std::{str::FromStr, sync::Arc, time::Duration};

use zksync_config::{
    configs::eth_sender::{PrecommitParams, ProofSendingMode, SenderConfig},
    ContractsConfig, EthConfig, GasAdjusterConfig,
};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::{
    clients::{DynClient, MockSettlementLayer, L1},
    BaseFees, BoundEthInterface,
};
use zksync_l1_contract_interface::i_executor::methods::{ExecuteBatches, ProveBatches};
use zksync_node_fee_model::l1_gas_price::{GasAdjuster, GasAdjusterClient};
use zksync_node_test_utils::{create_l1_batch, l1_batch_metadata_to_commitment_artifacts};
use zksync_object_store::MockObjectStore;
use zksync_types::{
    aggregated_operations::L1BatchAggregatedActionType, block::L1BatchHeader,
    commitment::L1BatchCommitmentMode, eth_sender::EthTx, pubdata_da::PubdataSendingMode,
    settlement::SettlementLayer, Address, L1BatchNumber, ProtocolVersion, ProtocolVersionId,
    SLChainId, H256,
};

use crate::{
    abstract_l1_interface::{L1BlockNumbers, OperatorType},
    aggregated_operations::{AggregatedOperation, L1BatchAggregatedOperation},
    tests::{default_l1_batch_metadata, l1_batch_with_metadata},
    Aggregator, EthTxAggregator, EthTxManager,
};

pub(super) const STATE_TRANSITION_CONTRACT_ADDRESS: Address = Address::repeat_byte(0xa0);
pub(super) const STATE_TRANSITION_MANAGER_CONTRACT_ADDRESS: Address = Address::repeat_byte(0xb0);

// Alias to conveniently call static methods of `ETHSender`.
type MockEthTxManager = EthTxManager;

pub(crate) struct TestL1Batch {
    pub number: L1BatchNumber,
}

impl TestL1Batch {
    pub async fn commit(&self, tester: &mut EthSenderTester, confirm: bool) -> H256 {
        assert_ne!(self.number, L1BatchNumber(0), "Cannot commit genesis batch");
        tester.commit_l1_batch(self.number, confirm).await
    }

    pub async fn save_commit_tx(&self, tester: &mut EthSenderTester) {
        assert_ne!(self.number, L1BatchNumber(0), "Cannot commit genesis batch");
        tester.save_commit_tx(self.number).await;
    }

    pub async fn prove(&self, tester: &mut EthSenderTester, confirm: bool) -> H256 {
        assert_ne!(self.number, L1BatchNumber(0), "Cannot prove genesis batch");
        tester.prove_l1_batch(self.number, confirm).await
    }

    pub async fn save_prove_tx(&self, tester: &mut EthSenderTester) {
        assert_ne!(self.number, L1BatchNumber(0), "Cannot commit genesis batch");
        tester.save_prove_tx(self.number).await;
    }
    pub async fn execute(&self, tester: &mut EthSenderTester, confirm: bool) -> H256 {
        assert_ne!(
            self.number,
            L1BatchNumber(0),
            "Cannot execute genesis batch"
        );
        tester.execute_l1_batch(self.number, confirm).await
    }

    pub async fn execute_commit_tx(&self, tester: &mut EthSenderTester) {
        tester
            .execute_tx(
                self.number,
                L1BatchAggregatedActionType::Commit,
                true,
                EthSenderTester::WAIT_CONFIRMATIONS,
            )
            .await;
    }

    pub async fn execute_prove_tx(&self, tester: &mut EthSenderTester) {
        tester
            .execute_tx(
                self.number,
                L1BatchAggregatedActionType::PublishProofOnchain,
                true,
                EthSenderTester::WAIT_CONFIRMATIONS,
            )
            .await;
    }

    pub async fn fail_commit_tx(&self, tester: &mut EthSenderTester) {
        tester
            .execute_tx(
                self.number,
                L1BatchAggregatedActionType::Commit,
                false,
                EthSenderTester::WAIT_CONFIRMATIONS,
            )
            .await;
    }

    pub async fn assert_commit_tx_just_sent(&self, tester: &mut EthSenderTester) {
        tester
            .assert_tx_was_sent_in_last_iteration(self.number, L1BatchAggregatedActionType::Commit)
            .await;
    }

    pub async fn sealed(tester: &mut EthSenderTester) -> Self {
        tester.seal_l1_batch().await;
        Self {
            number: tester.next_l1_batch_number_to_seal - 1,
        }
    }
}

#[derive(Debug)]
pub(crate) struct EthSenderTester {
    pub conn: ConnectionPool<Core>,
    pub gateway: Box<MockSettlementLayer>,
    pub gateway_blobs: Box<MockSettlementLayer>,
    pub l2_gateway: Box<MockSettlementLayer>,
    pub manager: MockEthTxManager,
    pub aggregator: EthTxAggregator,
    pub gas_adjuster: Arc<GasAdjuster>,
    pub pubdata_sending_mode: PubdataSendingMode,
    next_l1_batch_number_to_seal: L1BatchNumber,
    next_l1_batch_number_to_commit: L1BatchNumber,
    next_l1_batch_number_to_prove: L1BatchNumber,
    next_l1_batch_number_to_execute: L1BatchNumber,
    tx_sent_in_last_iteration_count: usize,
    pub settlement_layer: SettlementLayer,
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
        settlement_layer: SettlementLayer,
    ) -> Self {
        let eth_sender_config = EthConfig::for_tests();
        let contracts_config = ContractsConfig::for_tests();
        let pubdata_sending_mode =
            if aggregator_operate_4844_mode && commitment_mode == L1BatchCommitmentMode::Rollup {
                PubdataSendingMode::Blobs
            } else {
                PubdataSendingMode::Calldata
            };
        let aggregator_config = SenderConfig {
            pubdata_sending_mode,
            ..eth_sender_config
                .get_eth_sender_config_for_sender_layer_data_layer()
                .clone()
        };

        let history: Vec<_> = history
            .into_iter()
            .map(|base_fee_per_gas| BaseFees {
                base_fee_per_gas,
                base_fee_per_blob_gas: 1.into(),
                l2_pubdata_price: 0.into(),
            })
            .collect();

        let gateway = MockSettlementLayer::builder()
            .with_fee_history(
                std::iter::repeat_with(|| BaseFees {
                    base_fee_per_gas: 1,
                    base_fee_per_blob_gas: 1.into(),
                    l2_pubdata_price: 0.into(),
                })
                .take(Self::WAIT_CONFIRMATIONS as usize)
                .chain(history.clone())
                .collect(),
            )
            .with_non_ordering_confirmation(non_ordering_confirmations)
            .with_call_handler(move |call, _| {
                assert_eq!(call.to, Some(contracts_config.l1.multicall3_addr));
                crate::tests::mock_multicall_response(call)
            })
            .build();
        gateway.advance_block_number(Self::WAIT_CONFIRMATIONS);
        let gateway = Box::new(gateway);

        let chain_id = SLChainId(505);
        let l2_gateway: MockSettlementLayer = MockSettlementLayer::builder()
            .with_fee_history(
                std::iter::repeat_with(|| BaseFees {
                    base_fee_per_gas: 1,
                    base_fee_per_blob_gas: 1.into(),
                    l2_pubdata_price: 0.into(),
                })
                .take(Self::WAIT_CONFIRMATIONS as usize)
                .chain(history.clone())
                .collect(),
            )
            .with_non_ordering_confirmation(non_ordering_confirmations)
            .with_call_handler(move |call, _| {
                assert_eq!(call.to, Some(contracts_config.l1.multicall3_addr));
                crate::tests::mock_multicall_response(call)
            })
            .build();
        l2_gateway.advance_block_number(Self::WAIT_CONFIRMATIONS);
        let l2_gateway = Box::new(l2_gateway);

        let gateway_blobs = MockSettlementLayer::builder()
            .with_fee_history(
                std::iter::repeat_with(|| BaseFees {
                    base_fee_per_gas: 1,
                    base_fee_per_blob_gas: 1.into(),
                    l2_pubdata_price: 0.into(),
                })
                .take(Self::WAIT_CONFIRMATIONS as usize)
                .chain(history)
                .collect(),
            )
            .with_non_ordering_confirmation(non_ordering_confirmations)
            .with_call_handler(move |call, _| {
                assert_eq!(call.to, Some(contracts_config.l1.multicall3_addr));
                crate::tests::mock_multicall_response(call)
            })
            .with_sender(Address::from_str("0xb10b000000000000000000000000000000000000").unwrap())
            .build();
        gateway_blobs.advance_block_number(Self::WAIT_CONFIRMATIONS);
        let gateway_blobs = Box::new(gateway_blobs);

        let client: Box<DynClient<L1>> = Box::new(gateway.clone().into_client());
        let gas_adjuster = Arc::new(
            GasAdjuster::new(
                GasAdjusterClient::from(client),
                GasAdjusterConfig {
                    max_base_fee_samples: Self::MAX_BASE_FEE_SAMPLES,
                    pricing_formula_parameter_a: 3.0,
                    pricing_formula_parameter_b: 2.0,
                    ..eth_sender_config.gas_adjuster
                },
                pubdata_sending_mode,
                commitment_mode,
            )
            .await
            .unwrap(),
        );

        let eth_sender = eth_sender_config.get_eth_sender_config_for_sender_layer_data_layer();

        let use_blob_operator =
            aggregator_operate_4844_mode && commitment_mode == L1BatchCommitmentMode::Rollup;

        let aggregator = Aggregator::new(
            aggregator_config.clone(),
            MockObjectStore::arc(),
            use_blob_operator,
            commitment_mode,
            connection_pool.clone(),
            SettlementLayer::L1(chain_id),
            Some(PrecommitParams {
                l2_blocks_to_aggregate: 1,
                deadline: Duration::from_secs(1),
            }),
        )
        .await
        .unwrap();

        let aggregator = EthTxAggregator::new(
            connection_pool.clone(),
            SenderConfig {
                proof_sending_mode: ProofSendingMode::SkipEveryProof,
                pubdata_sending_mode,
                ..eth_sender.clone()
            },
            // Aggregator - unused
            aggregator,
            gateway.clone(),
            use_blob_operator.then(|| gateway_blobs.clone() as Box<dyn BoundEthInterface>),
            // ZKsync contract address
            Address::random(),
            STATE_TRANSITION_MANAGER_CONTRACT_ADDRESS,
            contracts_config.l1.multicall3_addr,
            STATE_TRANSITION_CONTRACT_ADDRESS,
            Default::default(),
            Some(SettlementLayer::L1(chain_id)),
        )
        .await;

        let manager = EthTxManager::new(
            connection_pool.clone(),
            eth_sender.clone(),
            gas_adjuster.clone(),
            Some(gateway.clone()),
            Some(gateway_blobs.clone()),
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
            gateway_blobs,
            l2_gateway,
            manager,
            aggregator,
            gas_adjuster,
            conn: connection_pool,
            pubdata_sending_mode,
            next_l1_batch_number_to_seal: L1BatchNumber(0),
            next_l1_batch_number_to_commit: L1BatchNumber(1),
            next_l1_batch_number_to_execute: L1BatchNumber(1),
            next_l1_batch_number_to_prove: L1BatchNumber(1),
            tx_sent_in_last_iteration_count: 0,
            settlement_layer,
        }
    }

    pub fn switch_to_using_gateway(&mut self) {
        self.manager = EthTxManager::new(
            self.conn.clone(),
            EthConfig::for_tests()
                .get_eth_sender_config_for_sender_layer_data_layer()
                .clone(),
            self.gas_adjuster.clone(),
            None,
            None,
            Some(self.l2_gateway.clone()),
        );
        self.settlement_layer = SettlementLayer::Gateway(10.into());
        tracing::info!("Switched eth-sender tester to use Gateway!");
    }

    pub async fn storage(&self) -> Connection<'_, Core> {
        self.conn.connection().await.unwrap()
    }

    pub async fn get_block_numbers(&self) -> L1BlockNumbers {
        let latest = self
            .manager
            .l1_interface()
            .get_l1_block_numbers(OperatorType::NonBlob)
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

    pub async fn execute_tx(
        &mut self,
        l1_batch_number: L1BatchNumber,
        operation_type: L1BatchAggregatedActionType,
        success: bool,
        confirmations: u64,
    ) {
        let tx = self
            .conn
            .connection()
            .await
            .unwrap()
            .eth_sender_dal()
            .get_last_sent_successfully_eth_tx_by_batch_and_op(l1_batch_number, operation_type)
            .await
            .unwrap();
        if !self.settlement_layer.is_gateway() {
            let (gateway, other) = if tx.blob_base_fee_per_gas.is_some() {
                (self.gateway_blobs.as_ref(), self.gateway.as_ref())
            } else {
                (self.gateway.as_ref(), self.gateway_blobs.as_ref())
            };
            gateway.execute_tx(tx.tx_hash, success, confirmations);
            other.advance_block_number(confirmations);
        } else {
            self.l2_gateway
                .execute_tx(tx.tx_hash, success, confirmations);
        }
    }

    pub async fn seal_l1_batch(&mut self) -> L1BatchHeader {
        let header = self
            .insert_l1_batch(self.next_l1_batch_number_to_seal)
            .await;
        self.next_l1_batch_number_to_seal += 1;
        header
    }

    pub async fn save_execute_tx(&mut self, l1_batch_number: L1BatchNumber) -> EthTx {
        assert_eq!(l1_batch_number, self.next_l1_batch_number_to_execute);
        let l1_batch_headers = vec![
            self.get_l1_batch_header_from_db(self.next_l1_batch_number_to_execute)
                .await,
        ];
        let operation =
            AggregatedOperation::L1Batch(L1BatchAggregatedOperation::Execute(ExecuteBatches {
                priority_ops_proofs: vec![Default::default(); l1_batch_headers.len()],
                l1_batches: l1_batch_headers
                    .into_iter()
                    .map(l1_batch_with_metadata)
                    .collect(),
            }));
        self.next_l1_batch_number_to_execute += 1;
        self.save_operation(operation).await
    }
    pub async fn execute_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
        confirm: bool,
    ) -> H256 {
        let tx = self.save_execute_tx(l1_batch_number).await;
        self.send_tx(tx, confirm).await
    }

    pub async fn save_prove_tx(&mut self, l1_batch_number: L1BatchNumber) -> EthTx {
        assert_eq!(l1_batch_number, self.next_l1_batch_number_to_prove);
        let operation = AggregatedOperation::L1Batch(
            L1BatchAggregatedOperation::PublishProofOnchain(ProveBatches {
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
            }),
        );
        self.next_l1_batch_number_to_prove += 1;
        self.save_operation(operation).await
    }

    pub async fn prove_l1_batch(&mut self, l1_batch_number: L1BatchNumber, confirm: bool) -> H256 {
        let tx = self.save_prove_tx(l1_batch_number).await;
        self.send_tx(tx, confirm).await
    }

    pub async fn run_eth_sender_tx_manager_iteration_after_n_blocks(&mut self, n: u64) {
        self.gateway.advance_block_number(n);
        self.gateway_blobs.advance_block_number(n);
        self.l2_gateway.advance_block_number(n);
        let tx_sent_before = self.gateway.sent_tx_count()
            + self.gateway_blobs.sent_tx_count()
            + self.l2_gateway.sent_tx_count();
        self.manager
            .loop_iteration(&mut self.conn.connection().await.unwrap())
            .await;
        self.tx_sent_in_last_iteration_count = (self.gateway.sent_tx_count()
            + self.gateway_blobs.sent_tx_count()
            + self.l2_gateway.sent_tx_count())
            - tx_sent_before;
    }

    pub async fn run_eth_sender_tx_manager_iteration(&mut self) {
        self.run_eth_sender_tx_manager_iteration_after_n_blocks(1)
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
    pub async fn save_commit_tx(&mut self, l1_batch_number: L1BatchNumber) -> EthTx {
        assert_eq!(l1_batch_number, self.next_l1_batch_number_to_commit);
        let pubdata_mode = if self.pubdata_sending_mode == PubdataSendingMode::Blobs {
            PubdataSendingMode::Blobs
        } else {
            PubdataSendingMode::Calldata
        };
        let commitment_mode = if self.pubdata_sending_mode == PubdataSendingMode::Custom {
            L1BatchCommitmentMode::Validium
        } else {
            L1BatchCommitmentMode::Rollup
        };

        let operation = AggregatedOperation::L1Batch(L1BatchAggregatedOperation::Commit(
            l1_batch_with_metadata(
                self.get_l1_batch_header_from_db(self.next_l1_batch_number_to_commit - 1)
                    .await,
            ),
            vec![l1_batch_with_metadata(
                self.get_l1_batch_header_from_db(self.next_l1_batch_number_to_commit)
                    .await,
            )],
            pubdata_mode,
            commitment_mode,
        ));
        self.next_l1_batch_number_to_commit += 1;
        self.save_operation(operation).await
    }

    pub async fn commit_l1_batch(&mut self, l1_batch_number: L1BatchNumber, confirm: bool) -> H256 {
        let tx = self.save_commit_tx(l1_batch_number).await;
        self.send_tx(tx, confirm).await
    }

    pub async fn save_operation(&mut self, aggregated_operation: AggregatedOperation) -> EthTx {
        self.aggregator
            .save_eth_tx(
                &mut self.conn.connection().await.unwrap(),
                &aggregated_operation,
                Address::random(),
                ProtocolVersionId::latest(),
                self.settlement_layer.is_gateway(),
            )
            .await
            .unwrap()
    }

    pub async fn send_tx(&mut self, tx: EthTx, confirm: bool) -> H256 {
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
            self.confirm_tx(hash, tx.blob_sidecar.is_some()).await;
        }
        hash
    }

    pub async fn confirm_tx(&mut self, hash: H256, is_blob: bool) {
        if !self.settlement_layer.is_gateway() {
            let (gateway, other) = if is_blob {
                (self.gateway_blobs.as_ref(), self.gateway.as_ref())
            } else {
                (self.gateway.as_ref(), self.gateway_blobs.as_ref())
            };
            gateway.execute_tx(hash, true, EthSenderTester::WAIT_CONFIRMATIONS);
            other.advance_block_number(EthSenderTester::WAIT_CONFIRMATIONS);
        } else {
            self.l2_gateway
                .execute_tx(hash, true, EthSenderTester::WAIT_CONFIRMATIONS);
        }
        self.run_eth_sender_tx_manager_iteration().await;
    }

    pub async fn assert_just_sent_tx_count_equals(&self, value: usize) {
        assert_eq!(
            value, self.tx_sent_in_last_iteration_count,
            "unexpected number of transactions sent in last tx manager iteration"
        )
    }

    pub async fn assert_tx_was_sent_in_last_iteration(
        &self,
        l1_batch_number: L1BatchNumber,
        operation_type: L1BatchAggregatedActionType,
    ) {
        let last_entry = self
            .conn
            .connection()
            .await
            .unwrap()
            .eth_sender_dal()
            .get_last_sent_successfully_eth_tx_by_batch_and_op(l1_batch_number, operation_type)
            .await
            .unwrap();
        let max_id = self
            .conn
            .connection()
            .await
            .unwrap()
            .eth_sender_dal()
            .get_eth_txs_history_entries_max_id()
            .await;
        assert!(
            max_id - self.tx_sent_in_last_iteration_count < last_entry.id as usize,
            "expected tx to be sent in last iteration, \
            max_id: {max_id}, \
            last_entry.id: {}, \
            txs sent in last iteration: {}",
            last_entry.id,
            self.tx_sent_in_last_iteration_count
        );
    }

    pub async fn assert_inflight_txs_count_equals(&mut self, value: usize) {
        let inflight_count = if !self.settlement_layer.is_gateway() {
            self.storage()
                .await
                .eth_sender_dal()
                .get_inflight_txs(
                    self.manager.operator_address(OperatorType::NonBlob),
                    false,
                    false,
                )
                .await
                .unwrap()
                .len()
                + self
                    .storage()
                    .await
                    .eth_sender_dal()
                    .get_inflight_txs(
                        self.manager.operator_address(OperatorType::Blob),
                        false,
                        false,
                    )
                    .await
                    .unwrap()
                    .len()
        } else {
            self.storage()
                .await
                .eth_sender_dal()
                .get_inflight_txs(
                    self.manager.operator_address(OperatorType::Gateway),
                    false,
                    true,
                )
                .await
                .unwrap()
                .len()
        };

        assert_eq!(
            inflight_count, value,
            "Unexpected number of in-flight transactions"
        );
    }
}
