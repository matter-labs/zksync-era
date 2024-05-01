//! Testing harness for the IO.

use std::{slice, sync::Arc, time::Duration};

use multivm::vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT;
use zksync_config::{
    configs::{chain::StateKeeperConfig, eth_sender::PubdataSendingMode, wallets::Wallets},
    GasAdjusterConfig,
};
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::clients::MockEthereum;
use zksync_types::{
    block::L2BlockHeader,
    fee::TransactionExecutionMetrics,
    fee_model::{BatchFeeInput, FeeModelConfig, FeeModelConfigV1},
    l2::L2Tx,
    protocol_version::L1VerifierConfig,
    system_contracts::get_system_smart_contracts,
    tx::TransactionExecutionResult,
    L2BlockNumber, L2ChainId, PriorityOpId, ProtocolVersionId, H256,
};

use crate::{
    fee_model::MainNodeFeeInputProvider,
    genesis::create_genesis_l1_batch,
    l1_gas_price::{GasAdjuster, PubdataPricing, RollupPubdataPricing, ValidiumPubdataPricing},
    state_keeper::{MempoolGuard, MempoolIO},
    utils::testonly::{
        create_l1_batch, create_l2_block, create_l2_transaction, execute_l2_transaction,
        DeploymentMode,
    },
};

#[derive(Debug)]
pub struct Tester {
    base_system_contracts: BaseSystemContracts,
    current_timestamp: u64,
    pubdata_pricing: Arc<dyn PubdataPricing>,
}

impl Tester {
    pub(super) fn new(deployment_mode: &DeploymentMode) -> Self {
        let base_system_contracts = BaseSystemContracts::load_from_disk();

        let pubdata_pricing: Arc<dyn PubdataPricing> = match deployment_mode {
            DeploymentMode::Validium => Arc::new(ValidiumPubdataPricing {}),
            DeploymentMode::Rollup => Arc::new(RollupPubdataPricing {}),
        };

        Self {
            base_system_contracts,
            current_timestamp: 0,
            pubdata_pricing,
        }
    }

    async fn create_gas_adjuster(&self) -> GasAdjuster {
        let eth_client =
            MockEthereum::default().with_fee_history(vec![0, 4, 6, 8, 7, 5, 5, 8, 10, 9]);

        let gas_adjuster_config = GasAdjusterConfig {
            default_priority_fee_per_gas: 10,
            max_base_fee_samples: 10,
            pricing_formula_parameter_a: 1.0,
            pricing_formula_parameter_b: 1.0,
            internal_l1_pricing_multiplier: 1.0,
            internal_enforced_l1_gas_price: None,
            internal_enforced_pubdata_price: None,
            poll_period: 10,
            max_l1_gas_price: None,
            num_samples_for_blob_base_fee_estimate: 10,
            internal_pubdata_pricing_multiplier: 1.0,
            max_blob_base_fee: None,
        };

        GasAdjuster::new(
            Arc::new(eth_client),
            gas_adjuster_config,
            PubdataSendingMode::Calldata,
            self.pubdata_pricing.clone(),
        )
        .await
        .unwrap()
    }

    pub(super) async fn create_batch_fee_input_provider(&self) -> MainNodeFeeInputProvider {
        let gas_adjuster = Arc::new(self.create_gas_adjuster().await);
        MainNodeFeeInputProvider::new(
            gas_adjuster,
            FeeModelConfig::V1(FeeModelConfigV1 {
                minimal_l2_gas_price: self.minimal_l2_gas_price(),
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
            FeeModelConfig::V1(FeeModelConfigV1 {
                minimal_l2_gas_price: self.minimal_l2_gas_price(),
            }),
        );

        let mempool = MempoolGuard::new(PriorityOpId(0), 100);
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
            wallets.state_keeper.unwrap().fee_account.address(),
            Duration::from_secs(1),
            L2ChainId::from(270),
        )
        .await
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
                ProtocolVersionId::latest(),
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
            .insert_transaction_l2(&tx, TransactionExecutionMetrics::default())
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
            )
            .await
            .unwrap();
        tx_result
    }

    pub(super) async fn insert_sealed_batch(
        &self,
        pool: &ConnectionPool<Core>,
        number: u32,
        tx_results: &[TransactionExecutionResult],
    ) {
        let batch_header = create_l1_batch(number);
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
            .mark_txs_as_executed_in_l1_batch(batch_header.number, tx_results)
            .await
            .unwrap();
        storage
            .blocks_dal()
            .set_l1_batch_hash(batch_header.number, H256::default())
            .await
            .unwrap();
    }

    pub(super) fn insert_tx(
        &self,
        guard: &mut MempoolGuard,
        fee_per_gas: u64,
        gas_per_pubdata: u32,
    ) -> L2Tx {
        let tx = create_l2_transaction(fee_per_gas, gas_per_pubdata.into());
        guard.insert(vec![tx.clone().into()], Default::default());
        tx
    }
}
