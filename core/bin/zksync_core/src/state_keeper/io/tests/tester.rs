//! Testing harness for the IO.

use crate::genesis::create_genesis_block;
use crate::l1_gas_price::GasAdjuster;
use crate::state_keeper::{MempoolGuard, MempoolIO};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use zksync_config::GasAdjusterConfig;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::ConnectionPool;
use zksync_eth_client::{clients::mock::MockEthereum, types::Error};
use zksync_mempool::MempoolStore;
use zksync_types::fee::Fee;
use zksync_types::l2::L2Tx;
use zksync_types::{
    block::{L1BatchHeader, MiniblockHeader},
    Address, L1BatchNumber, MiniblockNumber, PriorityOpId, H256,
};
use zksync_types::{L2ChainId, Nonce};

#[derive(Debug)]
pub(super) struct Tester {
    base_system_contracts: BaseSystemContracts,
}

impl Tester {
    pub(super) fn new() -> Self {
        let base_system_contracts = BaseSystemContracts::load_from_disk();
        Self {
            base_system_contracts,
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
    ) -> Result<(MempoolIO<GasAdjuster<MockEthereum>>, MempoolGuard), Error> {
        let gas_adjuster = Arc::new(self.create_gas_adjuster().await);

        let mempool = MempoolGuard(Arc::new(Mutex::new(MempoolStore::new(
            PriorityOpId(0),
            100,
        ))));

        Ok((
            MempoolIO::new(
                mempool.clone(),
                pool,
                Address::default(),
                self.fair_l2_gas_price(),
                Duration::from_secs(1),
                gas_adjuster,
                self.base_system_contracts.hashes(),
            ),
            mempool,
        ))
    }

    pub(super) fn genesis(&self, pool: &ConnectionPool) {
        let mut storage = pool.access_storage_blocking();
        if storage.blocks_dal().is_genesis_needed() {
            create_genesis_block(
                &mut storage,
                Address::repeat_byte(0x01),
                L2ChainId(270),
                self.base_system_contracts.clone(),
            );
        }
    }

    pub(super) fn insert_miniblock(
        &self,
        pool: &ConnectionPool,
        number: u32,
        base_fee_per_gas: u64,
        l1_gas_price: u64,
        l2_fair_gas_price: u64,
    ) {
        let mut storage = pool.access_storage_blocking();
        storage.blocks_dal().insert_miniblock(MiniblockHeader {
            number: MiniblockNumber(number),
            timestamp: 0,
            hash: Default::default(),
            l1_tx_count: 0,
            l2_tx_count: 0,
            base_fee_per_gas,
            l1_gas_price,
            l2_fair_gas_price,
            base_system_contracts_hashes: self.base_system_contracts.hashes(),
        });
    }

    pub(super) fn insert_sealed_batch(&self, pool: &ConnectionPool, number: u32) {
        let mut batch_header = L1BatchHeader::new(
            L1BatchNumber(number),
            0,
            Address::default(),
            self.base_system_contracts.hashes(),
        );
        batch_header.is_finished = true;

        let mut storage = pool.access_storage_blocking();

        storage
            .blocks_dal()
            .insert_l1_batch(batch_header.clone(), Default::default());

        storage
            .blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(batch_header.number);

        storage
            .blocks_dal()
            .set_l1_batch_hash(batch_header.number, H256::default());
    }

    pub(super) fn insert_tx(
        &self,
        guard: &mut MempoolGuard,
        fee_per_gas: u64,
        gas_per_pubdata: u32,
    ) {
        let fee = Fee {
            gas_limit: 1000u64.into(),
            max_fee_per_gas: fee_per_gas.into(),
            max_priority_fee_per_gas: 0u64.into(),
            gas_per_pubdata_limit: gas_per_pubdata.into(),
        };
        let mut tx = L2Tx::new_signed(
            Address::random(),
            vec![],
            Nonce(0),
            fee,
            Default::default(),
            L2ChainId(271),
            &H256::repeat_byte(0x11u8),
            None,
            Default::default(),
        )
        .unwrap();
        // Input means all transaction data (NOT calldata, but all tx fields) that came from the API.
        // This input will be used for the derivation of the tx hash, so put some random to it to be sure
        // that the transaction hash is unique.
        tx.set_input(H256::random().0.to_vec(), H256::random());

        guard.insert(vec![tx.into()], Default::default());
    }
}
