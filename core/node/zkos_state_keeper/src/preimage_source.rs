use zk_ee::system::ExecutionEnvironmentType;
use zk_ee::system::system_io_oracle::PreimageType;
use zk_ee::utils::Bytes32;
use zk_os_forward_system::run::PreimageSource;
use zksync_state::interface::ReadStorage;
use zksync_state::OwnedStorage;
use zksync_types::{L1BatchNumber, StorageKey, StorageValue, H256};
use zksync_utils::u256_to_h256;

pub struct ZkSyncPreimageSource {
    storage: OwnedStorage,
}

impl ZkSyncPreimageSource {
    pub fn new(storage: OwnedStorage) -> Self {
        Self { storage }
    }
}

impl PreimageSource for ZkSyncPreimageSource {
    fn get_preimage(&mut self, preimage_type: PreimageType, hash: Bytes32) -> Option<Vec<u8>> {
        match preimage_type {
            PreimageType::Bytecode(ExecutionEnvironmentType::EVM) => {
                self.storage.load_factory_dep(
                    //todo: endianness; also maybe change types?
                    H256::from_slice(&hash.as_u8_array())
                )
            }
            _ => unimplemented!("Preimage type is not supported"),
        }
    }
}