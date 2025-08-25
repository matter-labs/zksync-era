//! Testing harness for the IO.

use std::{slice, sync::Arc, time::Duration};

use zksync_config::{
    configs::{chain::StateKeeperConfig, wallets::Wallets},
    GasAdjusterConfig,
};
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::{
    clients::{DynClient, MockSettlementLayer, L1},
    BaseFees,
};
use zksync_multivm::{
    interface::{
        tracer::ValidationTraces, TransactionExecutionMetrics, TransactionExecutionResult,
    },
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};
use zksync_node_fee_model::{
    l1_gas_price::{GasAdjuster, GasAdjusterClient},
    MainNodeFeeInputProvider,
};
use zksync_node_genesis::create_genesis_l1_batch;
use zksync_node_test_utils::{
    create_l1_batch, create_l2_block, create_l2_transaction, execute_l2_transaction,
};
use zksync_types::{
    block::L2BlockHeader,
    commitment::L1BatchCommitmentMode,
    fee_model::{BaseTokenConversionRatio, BatchFeeInput, FeeModelConfig, FeeModelConfigV2},
    l2::L2Tx,
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    pubdata_da::PubdataSendingMode,
    settlement::SettlementLayer,
    system_contracts::get_system_smart_contracts,
    L2BlockNumber, L2ChainId, PriorityOpId, ProtocolVersionId, SLChainId,
    TransactionTimeRangeConstraint, H256,
};

use crate::{MempoolGuard, MempoolIO};

#[derive(Debug)]
pub struct Tester {
    base_system_contracts: BaseSystemContracts,
    current_timestamp: u64,
    commitment_mode: L1BatchCommitmentMode,
}

impl Tester {
    pub(super) fn new(commitment_mode: L1BatchCommitmentMode) -> Self {
        let base_system_contracts = BaseSystemContracts::load_from_disk();
        Self {
            base_system_contracts,
            current_timestamp: 0,
            commitment_mode,
        }
    }

    async fn create_gas_adjuster(&self) -> GasAdjuster {
        let block_fees = vec![0, 4, 6, 8, 7, 5, 5, 8, 10, 9];
        let base_fees = block_fees
            .into_iter()
            .map(|base_fee_per_gas| BaseFees {
                base_fee_per_gas,
                base_fee_per_blob_gas: 1.into(), // Not relevant for the test
                l2_pubdata_price: 0.into(),      // Not relevant for the test
            })
            .collect();
        let eth_client = MockSettlementLayer::builder()
            .with_fee_history(base_fees)
            .build();

        let gas_adjuster_config = GasAdjusterConfig {
            default_priority_fee_per_gas: 10,
            max_base_fee_samples: 10,
            pricing_formula_parameter_a: 1.0,
            pricing_formula_parameter_b: 1.0,
            internal_l1_pricing_multiplier: 1.0,
            internal_enforced_l1_gas_price: None,
            internal_enforced_pubdata_price: None,
            poll_period: Duration::from_millis(10),
            max_l1_gas_price: u64::MAX,
            num_samples_for_blob_base_fee_estimate: 10,
            internal_pubdata_pricing_multiplier: 1.0,
            max_blob_base_fee: u64::MAX,
        };

        let pool = ConnectionPool::<Core>::test_pool().await;

        let client: Box<DynClient<L1>> = Box::new(eth_client.into_client());

        GasAdjuster::new(
            GasAdjusterClient::from(client),
            gas_adjuster_config,
            PubdataSendingMode::Calldata,
            self.commitment_mode,
            pool,
        )
        .await
        .unwrap()
    }

    pub(super) async fn create_batch_fee_input_provider(&self) -> MainNodeFeeInputProvider {
        let gas_adjuster = Arc::new(self.create_gas_adjuster().await);

        MainNodeFeeInputProvider::new(
            gas_adjuster,
            Arc::<BaseTokenConversionRatio>::default(),
            FeeModelConfig::V2(FeeModelConfigV2 {
                minimal_l2_gas_price: self.minimal_l2_gas_price(),
                compute_overhead_part: 1.0,
                pubdata_overhead_part: 1.0,
                batch_overhead_l1_gas: 10,
                max_gas_per_batch: 500_000_000_000,
                max_pubdata_per_batch: 100_000_000_000,
            }),
        )
    }

    // Constant value to be used both in tests and inside of the IO.
    pub(super) fn minimal_l2_gas_price(&self) -> u64 {
        100
    }

    pub(super) async fn create_test_mempool_io(
        &self,
        pool: ConnectionPool<Core>,
    ) -> (MempoolIO, MempoolGuard) {
        let gas_adjuster = Arc::new(self.create_gas_adjuster().await);
        let batch_fee_input_provider = MainNodeFeeInputProvider::new(
            gas_adjuster,
            Arc::<BaseTokenConversionRatio>::default(),
            FeeModelConfig::V2(FeeModelConfigV2 {
                minimal_l2_gas_price: self.minimal_l2_gas_price(),
                compute_overhead_part: 1.0,
                pubdata_overhead_part: 1.0,
                batch_overhead_l1_gas: 10,
                max_gas_per_batch: 500_000_000_000,
                max_pubdata_per_batch: 100_000_000_000,
            }),
        );

        let chain_id = SLChainId(505);
        let mempool = MempoolGuard::new(PriorityOpId(0), 100, None, None);
        let config = StateKeeperConfig {
            minimal_l2_gas_price: self.minimal_l2_gas_price(),
            validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
            ..StateKeeperConfig::for_tests()
        };
        let wallets = Wallets::for_tests();
        let io = MempoolIO::new(
            mempool.clone(),
            Arc::new(batch_fee_input_provider),
            pool,
            &config,
            wallets.fee_account.unwrap().address(),
            Duration::from_secs(1),
            L2ChainId::from(270),
            Some(Default::default()),
            Default::default(),
            Some(SettlementLayer::L1(chain_id)),
        )
        .unwrap();

        (io, mempool)
    }

    pub(super) fn set_timestamp(&mut self, timestamp: u64) {
        self.current_timestamp = timestamp;
    }

    pub(super) async fn genesis(&self, pool: &ConnectionPool<Core>) {
        let mut storage = pool.connection_tagged("state_keeper").await.unwrap();
        if storage.blocks_dal().is_genesis_needed().await.unwrap() {
            create_genesis_l1_batch(
                &mut storage,
                ProtocolSemanticVersion {
                    minor: ProtocolVersionId::latest(),
                    patch: 0.into(),
                },
                &self.base_system_contracts,
                &get_system_smart_contracts(),
                L1VerifierConfig::default(),
            )
            .await
            .unwrap();
        }
    }

    pub(super) async fn insert_l2_block(
        &self,
        pool: &ConnectionPool<Core>,
        number: u32,
        base_fee_per_gas: u64,
        fee_input: BatchFeeInput,
    ) -> TransactionExecutionResult {
        let mut storage = pool.connection_tagged("state_keeper").await.unwrap();
        let tx = create_l2_transaction(10, 100);
        storage
            .transactions_dal()
            .insert_transaction_l2(
                &tx,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        storage
            .blocks_dal()
            .insert_l2_block(&L2BlockHeader {
                timestamp: self.current_timestamp,
                base_fee_per_gas,
                batch_fee_input: fee_input,
                base_system_contracts_hashes: self.base_system_contracts.hashes(),
                ..create_l2_block(number)
            })
            .await
            .unwrap();
        let tx_result = execute_l2_transaction(tx.clone());
        storage
            .transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                L2BlockNumber(number),
                slice::from_ref(&tx_result),
                1.into(),
                ProtocolVersionId::latest(),
                false,
            )
            .await
            .unwrap();
        tx_result
    }

    pub(super) async fn insert_sealed_batch(
        &self,
        pool: &ConnectionPool<Core>,
        number: u32,
        tx_hashes: &[H256],
    ) {
        let mut batch_header = create_l1_batch(number);
        batch_header.timestamp = self.current_timestamp;
        let mut storage = pool.connection_tagged("state_keeper").await.unwrap();
        storage
            .blocks_dal()
            .insert_mock_l1_batch(&batch_header)
            .await
            .unwrap();
        storage
            .blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(batch_header.number)
            .await
            .unwrap();
        storage
            .transactions_dal()
            .mark_txs_as_executed_in_l1_batch(batch_header.number, tx_hashes)
            .await
            .unwrap();
        storage
            .blocks_dal()
            .set_l1_batch_hash(batch_header.number, H256::default())
            .await
            .unwrap();
    }

    pub(super) async fn insert_unsealed_batch(
        &self,
        pool: &ConnectionPool<Core>,
        number: u32,
        fee_input: BatchFeeInput,
    ) {
        let mut batch_header = create_l1_batch(number);
        batch_header.batch_fee_input = fee_input;
        let mut storage = pool.connection_tagged("state_keeper").await.unwrap();
        storage
            .blocks_dal()
            .insert_l1_batch(batch_header.to_unsealed_header())
            .await
            .unwrap();
    }

    pub(super) fn insert_tx(
        &self,
        guard: &mut MempoolGuard,
        fee_per_gas: u64,
        gas_per_pubdata: u32,
        constraint: TransactionTimeRangeConstraint,
    ) -> L2Tx {
        let tx = create_l2_transaction(fee_per_gas, gas_per_pubdata.into());
        guard.insert(vec![(tx.clone().into(), constraint)], Default::default());
        tx
    }
}
