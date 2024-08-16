use std::{cell::RefCell, rc::Rc};

use vm2::WorldDiff;
use zksync_contracts::BaseSystemContracts;
use zksync_test_account::{Account, TxType};
use zksync_types::{
    block::L2BlockHasher,
    fee_model::BatchFeeInput,
    get_code_key, get_is_account_key,
    helpers::unix_timestamp_ms,
    utils::{deployed_address_create, storage_key_for_eth_balance},
    AccountTreeId, Address, L1BatchNumber, L2BlockNumber, L2ChainId, Nonce, ProtocolVersionId,
    StorageKey, U256,
};
use zksync_utils::{bytecode::hash_bytecode, u256_to_h256};

use crate::{
    interface::{
        storage::{InMemoryStorage, StoragePtr},
        L1BatchEnv, L2Block, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode, VmInterface,
    },
    versions::vm_fast::{tests::utils::read_test_contract, vm::Vm},
    vm_latest::{constants::BATCH_COMPUTATIONAL_GAS_LIMIT, utils::l2_blocks::load_last_l2_block},
};

pub(crate) struct VmTester {
    pub(crate) vm: Vm<StoragePtr<InMemoryStorage>>,
    pub(crate) storage: StoragePtr<InMemoryStorage>,
    pub(crate) deployer: Option<Account>,
    pub(crate) test_contract: Option<Address>,
    pub(crate) fee_account: Address,
    pub(crate) rich_accounts: Vec<Account>,
    pub(crate) custom_contracts: Vec<ContractsToDeploy>,
}

impl VmTester {
    pub(crate) fn deploy_test_contract(&mut self) {
        let contract = read_test_contract();
        let tx = self
            .deployer
            .as_mut()
            .expect("You have to initialize builder with deployer")
            .get_deploy_tx(&contract, None, TxType::L2)
            .tx;
        let nonce = tx.nonce().unwrap().0.into();
        self.vm.push_transaction(tx);
        self.vm.execute(VmExecutionMode::OneTx);
        let deployed_address =
            deployed_address_create(self.deployer.as_ref().unwrap().address, nonce);
        self.test_contract = Some(deployed_address);
    }

    pub(crate) fn reset_with_empty_storage(&mut self) {
        self.storage = Rc::new(RefCell::new(get_empty_storage()));
        self.vm.inner.world_diff = WorldDiff::default();
        self.reset_state(false);
    }

    /// Reset the state of the VM to the initial state.
    /// If `use_latest_l2_block` is true, then the VM will use the latest L2 block from storage,
    /// otherwise it will use the first L2 block of l1 batch env
    pub(crate) fn reset_state(&mut self, use_latest_l2_block: bool) {
        for account in self.rich_accounts.iter_mut() {
            account.nonce = Nonce(0);
            make_account_rich(self.storage.clone(), account);
        }
        if let Some(deployer) = &self.deployer {
            make_account_rich(self.storage.clone(), deployer);
        }

        if !self.custom_contracts.is_empty() {
            println!("Inserting custom contracts is not yet supported")
            // `insert_contracts(&mut self.storage, &self.custom_contracts);`
        }

        let storage = self.storage.clone();
        {
            let mut storage = storage.borrow_mut();
            // Commit pending storage changes (old VM versions commit them on successful execution)
            for (&(address, slot), &value) in self.vm.inner.world_diff.get_storage_state() {
                let key = StorageKey::new(AccountTreeId::new(address), u256_to_h256(slot));
                storage.set_value(key, u256_to_h256(value));
            }
        }

        let mut l1_batch = self.vm.batch_env.clone();
        if use_latest_l2_block {
            let last_l2_block = load_last_l2_block(&storage).unwrap_or(L2Block {
                number: 0,
                timestamp: 0,
                hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
            });
            l1_batch.first_l2_block = L2BlockEnv {
                number: last_l2_block.number + 1,
                timestamp: std::cmp::max(last_l2_block.timestamp + 1, l1_batch.timestamp),
                prev_block_hash: last_l2_block.hash,
                max_virtual_blocks_to_create: 1,
            };
        }

        let vm = Vm::new(l1_batch, self.vm.system_env.clone(), storage);

        if self.test_contract.is_some() {
            self.deploy_test_contract();
        }
        self.vm = vm;
    }
}

pub(crate) type ContractsToDeploy = (Vec<u8>, Address, bool);

pub(crate) struct VmTesterBuilder {
    storage: Option<InMemoryStorage>,
    l1_batch_env: Option<L1BatchEnv>,
    system_env: SystemEnv,
    deployer: Option<Account>,
    rich_accounts: Vec<Account>,
    custom_contracts: Vec<ContractsToDeploy>,
}

impl Clone for VmTesterBuilder {
    fn clone(&self) -> Self {
        Self {
            storage: None,
            l1_batch_env: self.l1_batch_env.clone(),
            system_env: self.system_env.clone(),
            deployer: self.deployer.clone(),
            rich_accounts: self.rich_accounts.clone(),
            custom_contracts: self.custom_contracts.clone(),
        }
    }
}

#[allow(dead_code)]
impl VmTesterBuilder {
    pub(crate) fn new() -> Self {
        Self {
            storage: None,
            l1_batch_env: None,
            system_env: SystemEnv {
                zk_porter_available: false,
                version: ProtocolVersionId::latest(),
                base_system_smart_contracts: BaseSystemContracts::playground(),
                bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
                execution_mode: TxExecutionMode::VerifyExecute,
                default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
                chain_id: L2ChainId::from(270),
                pubdata_params: Default::default(),
            },
            deployer: None,
            rich_accounts: vec![],
            custom_contracts: vec![],
        }
    }

    pub(crate) fn with_l1_batch_env(mut self, l1_batch_env: L1BatchEnv) -> Self {
        self.l1_batch_env = Some(l1_batch_env);
        self
    }

    pub(crate) fn with_system_env(mut self, system_env: SystemEnv) -> Self {
        self.system_env = system_env;
        self
    }

    pub(crate) fn with_storage(mut self, storage: InMemoryStorage) -> Self {
        self.storage = Some(storage);
        self
    }

    pub(crate) fn with_base_system_smart_contracts(
        mut self,
        base_system_smart_contracts: BaseSystemContracts,
    ) -> Self {
        self.system_env.base_system_smart_contracts = base_system_smart_contracts;
        self
    }

    pub(crate) fn with_bootloader_gas_limit(mut self, gas_limit: u32) -> Self {
        self.system_env.bootloader_gas_limit = gas_limit;
        self
    }

    pub(crate) fn with_execution_mode(mut self, execution_mode: TxExecutionMode) -> Self {
        self.system_env.execution_mode = execution_mode;
        self
    }

    pub(crate) fn with_empty_in_memory_storage(mut self) -> Self {
        self.storage = Some(get_empty_storage());
        self
    }

    pub(crate) fn with_random_rich_accounts(mut self, number: u32) -> Self {
        for _ in 0..number {
            let account = Account::random();
            self.rich_accounts.push(account);
        }
        self
    }

    pub(crate) fn with_rich_accounts(mut self, accounts: Vec<Account>) -> Self {
        self.rich_accounts.extend(accounts);
        self
    }

    pub(crate) fn with_deployer(mut self) -> Self {
        let deployer = Account::random();
        self.deployer = Some(deployer);
        self
    }

    pub(crate) fn with_custom_contracts(mut self, contracts: Vec<ContractsToDeploy>) -> Self {
        self.custom_contracts = contracts;
        self
    }

    pub(crate) fn build(self) -> VmTester {
        let l1_batch_env = self
            .l1_batch_env
            .unwrap_or_else(|| default_l1_batch(L1BatchNumber(1)));

        let mut raw_storage = self.storage.unwrap_or_else(get_empty_storage);
        insert_contracts(&mut raw_storage, &self.custom_contracts);
        let storage_ptr = Rc::new(RefCell::new(raw_storage));
        for account in self.rich_accounts.iter() {
            make_account_rich(storage_ptr.clone(), account);
        }
        if let Some(deployer) = &self.deployer {
            make_account_rich(storage_ptr.clone(), deployer);
        }

        let fee_account = l1_batch_env.fee_account;
        let vm = Vm::new(l1_batch_env, self.system_env, storage_ptr.clone());

        VmTester {
            vm,
            storage: storage_ptr,
            deployer: self.deployer,
            test_contract: None,
            fee_account,
            rich_accounts: self.rich_accounts.clone(),
            custom_contracts: self.custom_contracts.clone(),
        }
    }
}

pub(crate) fn default_l1_batch(number: L1BatchNumber) -> L1BatchEnv {
    let timestamp = unix_timestamp_ms();
    L1BatchEnv {
        previous_batch_hash: None,
        number,
        timestamp,
        fee_input: BatchFeeInput::l1_pegged(
            50_000_000_000, // 50 gwei
            250_000_000,    // 0.25 gwei
        ),
        fee_account: Address::random(),
        enforced_base_fee: None,
        first_l2_block: L2BlockEnv {
            number: 1,
            timestamp,
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
            max_virtual_blocks_to_create: 100,
        },
    }
}

pub(crate) fn make_account_rich(storage: StoragePtr<InMemoryStorage>, account: &Account) {
    let key = storage_key_for_eth_balance(&account.address);
    storage
        .as_ref()
        .borrow_mut()
        .set_value(key, u256_to_h256(U256::from(10u64.pow(19))));
}

pub(crate) fn get_empty_storage() -> InMemoryStorage {
    InMemoryStorage::with_system_contracts(hash_bytecode)
}

// Inserts the contracts into the test environment, bypassing the
// deployer system contract. Besides the reference to storage
// it accepts a `contracts` tuple of information about the contract
// and whether or not it is an account.
fn insert_contracts(raw_storage: &mut InMemoryStorage, contracts: &[ContractsToDeploy]) {
    for (contract, address, is_account) in contracts {
        let deployer_code_key = get_code_key(address);
        raw_storage.set_value(deployer_code_key, hash_bytecode(contract));

        if *is_account {
            let is_account_key = get_is_account_key(address);
            raw_storage.set_value(is_account_key, u256_to_h256(1_u32.into()));
        }

        raw_storage.store_factory_dep(hash_bytecode(contract), contract.clone());
    }
}
