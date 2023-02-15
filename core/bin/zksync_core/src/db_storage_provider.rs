use zksync_dal::StorageProcessor;
use zksync_types::{Address, MiniblockNumber, StorageKey, StorageValue, ZkSyncReadStorage, H256};

#[derive(Debug)]
pub struct DbStorageProvider<'a> {
    connection: StorageProcessor<'a>,
    block_number: MiniblockNumber,
    consider_new_l1_batch: bool,
}

impl<'a> DbStorageProvider<'a> {
    pub fn new(
        connection: StorageProcessor<'a>,
        block_number: MiniblockNumber,
        consider_new_l1_batch: bool,
    ) -> DbStorageProvider<'a> {
        DbStorageProvider {
            connection,
            block_number,
            consider_new_l1_batch,
        }
    }
}

impl<'a> ZkSyncReadStorage for DbStorageProvider<'a> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        self.connection
            .storage_web3_dal()
            .get_historical_value_unchecked(key, self.block_number)
            .unwrap()
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.connection
            .storage_web3_dal()
            .is_write_initial(key, self.block_number, self.consider_new_l1_batch)
            .unwrap()
    }

    fn load_contract(&mut self, address: Address) -> Option<Vec<u8>> {
        self.connection
            .storage_web3_dal()
            .get_contract_code_unchecked(address, self.block_number)
            .unwrap()
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.connection
            .storage_web3_dal()
            .get_factory_dep_unchecked(hash, self.block_number)
            .unwrap()
    }
}
