use std::{cell::RefCell, rc::Rc};

use vm2::WorldDiff;
use zksync_contracts::BaseSystemContracts;
use zksync_test_account::{Account, TxType};
use zksync_types::{
    block::L2BlockHasher, utils::deployed_address_create, AccountTreeId, Address, L1BatchNumber,
    L2BlockNumber, Nonce, StorageKey,
};
use zksync_utils::{bytecode::hash_bytecode, u256_to_h256};

use crate::{
    interface::{
        storage::{InMemoryStorage, StoragePtr},
        L1BatchEnv, L2Block, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode, VmInterface,
    },
    versions::{
        testonly::{default_l1_batch, default_system_env, make_account_rich, ContractToDeploy},
        vm_fast::{tests::utils::read_test_contract, vm::Vm},
    },
    vm_latest::utils::l2_blocks::load_last_l2_block,
};

pub(crate) struct VmTester {
    pub(crate) vm: Vm<StoragePtr<InMemoryStorage>>,
    pub(crate) storage: StoragePtr<InMemoryStorage>,
    pub(crate) deployer: Option<Account>,
    pub(crate) test_contract: Option<Address>,
    pub(crate) fee_account: Address,
    pub(crate) rich_accounts: Vec<Account>,
    pub(crate) custom_contracts: Vec<ContractToDeploy>,
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
            make_account_rich(&mut self.storage.borrow_mut(), account);
        }
        if let Some(deployer) = &self.deployer {
            make_account_rich(&mut self.storage.borrow_mut(), deployer);
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

        let vm = Vm::custom(l1_batch, self.vm.system_env.clone(), storage);

        if self.test_contract.is_some() {
            self.deploy_test_contract();
        }
        self.vm = vm;
    }
}

pub(crate) struct VmTesterBuilder {
    storage: Option<InMemoryStorage>,
    l1_batch_env: Option<L1BatchEnv>,
    system_env: SystemEnv,
    deployer: Option<Account>,
    rich_accounts: Vec<Account>,
    custom_contracts: Vec<ContractToDeploy>,
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

impl VmTesterBuilder {
    pub(crate) fn new() -> Self {
        Self {
            storage: None,
            l1_batch_env: None,
            system_env: default_system_env(),
            deployer: None,
            rich_accounts: vec![],
            custom_contracts: vec![],
        }
    }

    pub(crate) fn with_l1_batch_env(mut self, l1_batch_env: L1BatchEnv) -> Self {
        self.l1_batch_env = Some(l1_batch_env);
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

    pub(crate) fn with_custom_contracts(mut self, contracts: Vec<ContractToDeploy>) -> Self {
        self.custom_contracts = contracts;
        self
    }

    pub(crate) fn build(self) -> VmTester {
        let l1_batch_env = self
            .l1_batch_env
            .unwrap_or_else(|| default_l1_batch(L1BatchNumber(1)));

        let mut raw_storage = self.storage.unwrap_or_else(get_empty_storage);
        ContractToDeploy::insert(&self.custom_contracts, &mut raw_storage);
        let storage_ptr = Rc::new(RefCell::new(raw_storage));
        for account in self.rich_accounts.iter() {
            make_account_rich(&mut storage_ptr.borrow_mut(), account);
        }
        if let Some(deployer) = &self.deployer {
            make_account_rich(&mut storage_ptr.borrow_mut(), deployer);
        }

        let fee_account = l1_batch_env.fee_account;
        let vm = Vm::custom(l1_batch_env, self.system_env, storage_ptr.clone());

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

pub(crate) fn get_empty_storage() -> InMemoryStorage {
    InMemoryStorage::with_system_contracts(hash_bytecode)
}
