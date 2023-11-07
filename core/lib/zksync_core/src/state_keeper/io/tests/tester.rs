//! Testing harness for the IO.

use multivm::vm_latest::constants::BLOCK_GAS_LIMIT;
use std::{sync::Arc, time::Duration};
use zksync_object_store::ObjectStoreFactory;

use zksync_config::configs::chain::StateKeeperConfig;
use zksync_config::GasAdjusterConfig;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::ConnectionPool;
use zksync_eth_client::clients::mock::MockEthereum;
use zksync_types::{
    block::{L1BatchHeader, MiniblockHeader},
    protocol_version::L1VerifierConfig,
    system_contracts::get_system_smart_contracts,
    Address, L1BatchNumber, L2ChainId, MiniblockNumber, PriorityOpId, ProtocolVersionId, H256,
};

use crate::{
    genesis::create_genesis_l1_batch,
    l1_gas_price::GasAdjuster,
    state_keeper::{io::MiniblockSealer, tests::create_transaction, MempoolGuard, MempoolIO},
};

#[derive(Debug)]
pub(super) struct Tester {
    base_system_contracts: BaseSystemContracts,
    current_timestamp: u64,
}

impl Tester {
    pub(super) fn new() -> Self {
        let base_system_contracts = BaseSystemContracts::load_from_disk();
        Self {
            base_system_contracts,
            current_timestamp: 0,
        }
    }

    pub(super) async fn create_gas_adjuster(&self) -> GasAdjuster<MockEthereum> {
        let eth_client =
            MockEthereum::default().with_fee_history(vec![0, 4, 6, 8, 7, 5, 5, 8, 10, 9]);

        let gas_adjuster_config = GasAdjusterConfig {
            default_priority_fee_per_gas: 10,
            max_base_fee_samples: 10,
            pricing_formula_parameter_a: 1.0,
            pricing_formula_parameter_b: 1.0,
            internal_l1_pricing_multiplier: 1.0,
            internal_enforced_l1_gas_price: None,
            poll_period: 10,
            max_l1_gas_price: None,
        };

        GasAdjuster::new(eth_client, gas_adjuster_config)
            .await
            .unwrap()
    }

    // Constant value to be used both in tests and inside of the IO.
    pub(super) fn fair_l2_gas_price(&self) -> u64 {
        100
    }

    pub(super) async fn create_test_mempool_io(
        &self,
        pool: ConnectionPool,
        miniblock_sealer_capacity: usize,
    ) -> (MempoolIO<GasAdjuster<MockEthereum>>, MempoolGuard) {
        let gas_adjuster = Arc::new(self.create_gas_adjuster().await);
        let mempool = MempoolGuard::new(PriorityOpId(0), 100);
        let (miniblock_sealer, miniblock_sealer_handle) =
            MiniblockSealer::new(pool.clone(), miniblock_sealer_capacity);
        tokio::spawn(miniblock_sealer.run());

        let config = StateKeeperConfig {
            fair_l2_gas_price: self.fair_l2_gas_price(),
            virtual_blocks_interval: 1,
            virtual_blocks_per_miniblock: 1,
            ..StateKeeperConfig::default()
        };
        let object_store = ObjectStoreFactory::mock().create_store().await;
        let l2_erc20_bridge_addr = Address::repeat_byte(0x5a); // Isn't relevant.
        let io = MempoolIO::new(
            mempool.clone(),
            object_store,
            miniblock_sealer_handle,
            gas_adjuster,
            pool,
            &config,
            Duration::from_secs(1),
            l2_erc20_bridge_addr,
            BLOCK_GAS_LIMIT,
            L2ChainId::from(270),
        )
        .await;

        (io, mempool)
    }

    pub(super) fn set_timestamp(&mut self, timestamp: u64) {
        self.current_timestamp = timestamp;
    }

    pub(super) async fn genesis(&self, pool: &ConnectionPool) {
        let mut storage = pool.access_storage_tagged("state_keeper").await.unwrap();
        if storage.blocks_dal().is_genesis_needed().await.unwrap() {
            create_genesis_l1_batch(
                &mut storage,
                Address::repeat_byte(0x01),
                L2ChainId::from(270),
                ProtocolVersionId::latest(),
                &self.base_system_contracts,
                &get_system_smart_contracts(),
                L1VerifierConfig::default(),
                Address::zero(),
            )
            .await;
        }
    }

    pub(super) async fn insert_miniblock(
        &self,
        pool: &ConnectionPool,
        number: u32,
        base_fee_per_gas: u64,
        l1_gas_price: u64,
        l2_fair_gas_price: u64,
    ) {
        let mut storage = pool.access_storage_tagged("state_keeper").await.unwrap();
        storage
            .blocks_dal()
            .insert_miniblock(&MiniblockHeader {
                number: MiniblockNumber(number),
                timestamp: self.current_timestamp,
                hash: H256::default(),
                l1_tx_count: 0,
                l2_tx_count: 0,
                base_fee_per_gas,
                l1_gas_price,
                l2_fair_gas_price,
                base_system_contracts_hashes: self.base_system_contracts.hashes(),
                protocol_version: Some(ProtocolVersionId::latest()),
                virtual_blocks: 0,
            })
            .await
            .unwrap();
    }

    pub(super) async fn insert_sealed_batch(&self, pool: &ConnectionPool, number: u32) {
        let mut batch_header = L1BatchHeader::new(
            L1BatchNumber(number),
            self.current_timestamp,
            Address::default(),
            self.base_system_contracts.hashes(),
            Default::default(),
        );
        batch_header.is_finished = true;

        let mut storage = pool.access_storage_tagged("state_keeper").await.unwrap();
        storage
            .blocks_dal()
            .insert_l1_batch(&batch_header, &[], Default::default(), &[], &[])
            .await
            .unwrap();
        storage
            .blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(batch_header.number)
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
    ) {
        let tx = create_transaction(fee_per_gas, gas_per_pubdata);
        guard.insert(vec![tx], Default::default());
    }
}
