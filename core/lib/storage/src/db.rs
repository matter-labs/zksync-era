use once_cell::sync::Lazy;
use rocksdb::{
    AsColumnFamilyRef, BlockBasedOptions, ColumnFamily, ColumnFamilyDescriptor, Options,
    WriteBatch, DB,
};
use std::path::Path;
use std::sync::{Condvar, Mutex};

/// Number of active RocksDB instances
/// Used to determine if it's safe to exit current process
/// Not properly dropped rocksdb instances can lead to db corruption
#[allow(clippy::mutex_atomic)]
pub(crate) static ROCKSDB_INSTANCE_COUNTER: Lazy<(Mutex<usize>, Condvar)> =
    Lazy::new(|| (Mutex::new(0), Condvar::new()));

/// Thin wrapper around RocksDB
#[derive(Debug)]
pub struct RocksDB {
    db: DB,
    _registry_entry: RegistryEntry,
}

#[derive(Debug)]
pub enum Database {
    MerkleTree,
    StateKeeper,
}

#[derive(Debug)]
pub enum MerkleTreeColumnFamily {
    Tree,
    LeafIndices,
}

#[derive(Debug)]
pub enum StateKeeperColumnFamily {
    State,
    Contracts,
    FactoryDeps,
}

impl MerkleTreeColumnFamily {
    fn all() -> &'static [Self] {
        &[Self::Tree, Self::LeafIndices]
    }
}

impl StateKeeperColumnFamily {
    fn all() -> &'static [Self] {
        &[Self::State, Self::Contracts, Self::FactoryDeps]
    }
}

impl std::fmt::Display for MerkleTreeColumnFamily {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let value = match self {
            MerkleTreeColumnFamily::Tree => "default",
            MerkleTreeColumnFamily::LeafIndices => "leaf_indices",
        };
        write!(formatter, "{}", value)
    }
}

impl std::fmt::Display for StateKeeperColumnFamily {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let value = match self {
            StateKeeperColumnFamily::State => "state",
            StateKeeperColumnFamily::Contracts => "contracts",
            StateKeeperColumnFamily::FactoryDeps => "factory_deps",
        };
        write!(formatter, "{}", value)
    }
}

impl RocksDB {
    pub fn new<P: AsRef<Path>>(database: Database, path: P, tune_options: bool) -> Self {
        let options = Self::rocksdb_options(tune_options);
        let db = match database {
            Database::MerkleTree => {
                let cfs = MerkleTreeColumnFamily::all().iter().map(|cf| {
                    ColumnFamilyDescriptor::new(cf.to_string(), Self::rocksdb_options(tune_options))
                });
                DB::open_cf_descriptors(&options, path, cfs).expect("failed to init rocksdb")
            }
            Database::StateKeeper => {
                let cfs = StateKeeperColumnFamily::all().iter().map(|cf| {
                    ColumnFamilyDescriptor::new(cf.to_string(), Self::rocksdb_options(tune_options))
                });
                DB::open_cf_descriptors(&options, path, cfs).expect("failed to init rocksdb")
            }
        };

        Self {
            db,
            _registry_entry: RegistryEntry::new(),
        }
    }

    fn rocksdb_options(tune_options: bool) -> Options {
        let mut options = Options::default();
        options.create_missing_column_families(true);
        options.create_if_missing(true);
        if tune_options {
            options.increase_parallelism(num_cpus::get() as i32);
            let mut block_based_options = BlockBasedOptions::default();
            block_based_options.set_bloom_filter(10.0, false);
            options.set_block_based_table_factory(&block_based_options);
        }
        options
    }

    pub fn get_estimated_number_of_entries(&self, cf: StateKeeperColumnFamily) -> u64 {
        let error_msg = "failed to get estimated number of entries";
        let cf = self.db.cf_handle(&cf.to_string()).unwrap();
        self.db
            .property_int_value_cf(cf, "rocksdb.estimate-num-keys")
            .expect(error_msg)
            .expect(error_msg)
    }

    pub fn multi_get<K, I>(&self, keys: I) -> Vec<Result<Option<Vec<u8>>, rocksdb::Error>>
    where
        K: AsRef<[u8]>,
        I: IntoIterator<Item = K>,
    {
        self.db.multi_get(keys)
    }

    pub fn multi_get_cf<'a, 'b: 'a, K, I, W: 'b>(
        &'a self,
        keys: I,
    ) -> Vec<Result<Option<Vec<u8>>, rocksdb::Error>>
    where
        K: AsRef<[u8]>,
        I: IntoIterator<Item = (&'b W, K)>,
        W: AsColumnFamilyRef,
    {
        self.db.multi_get_cf(keys)
    }

    pub fn write(&self, batch: WriteBatch) -> Result<(), rocksdb::Error> {
        self.db.write(batch)
    }

    pub fn put<K, V>(&self, key: K, value: V) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.db.put(key, value)
    }

    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Vec<u8>>, rocksdb::Error> {
        self.db.get(key)
    }

    /// Returns column family handle for State Keeper database
    pub fn cf_state_keeper_handle(&self, cf: StateKeeperColumnFamily) -> &ColumnFamily {
        self.db
            .cf_handle(&cf.to_string())
            .unwrap_or_else(|| panic!("Column family '{}' doesn't exist", cf))
    }

    /// Returns column family handle for Merkle Tree database
    pub fn cf_merkle_tree_handle(&self, cf: MerkleTreeColumnFamily) -> &ColumnFamily {
        self.db
            .cf_handle(&cf.to_string())
            .unwrap_or_else(|| panic!("Column family '{}' doesn't exist", cf))
    }

    pub fn get_cf<K: AsRef<[u8]>>(
        &self,
        cf: &impl AsColumnFamilyRef,
        key: K,
    ) -> Result<Option<Vec<u8>>, rocksdb::Error> {
        self.db.get_cf(cf, key)
    }

    /// awaits termination of all running rocksdb instances
    pub fn await_rocksdb_termination() {
        let (lock, cvar) = &*ROCKSDB_INSTANCE_COUNTER;
        let mut num_instances = lock.lock().unwrap();
        while *num_instances != 0 {
            vlog::info!(
                "Waiting for all the RocksDB instances to be dropped, {} remaining",
                *num_instances
            );
            num_instances = cvar.wait(num_instances).unwrap();
        }
        vlog::info!("All the RocksDB instances are dropped");
    }
}

impl Drop for RocksDB {
    fn drop(&mut self) {
        self.db.cancel_all_background_work(true);
    }
}

/// Empty struct used to register rocksdb instance
#[derive(Debug)]
struct RegistryEntry;

impl RegistryEntry {
    fn new() -> Self {
        let (lock, cvar) = &*ROCKSDB_INSTANCE_COUNTER;
        let mut num_instances = lock.lock().unwrap();
        *num_instances += 1;
        cvar.notify_all();
        Self
    }
}

impl Drop for RegistryEntry {
    fn drop(&mut self) {
        let (lock, cvar) = &*ROCKSDB_INSTANCE_COUNTER;
        let mut num_instances = lock.lock().unwrap();
        *num_instances -= 1;
        cvar.notify_all();
    }
}
