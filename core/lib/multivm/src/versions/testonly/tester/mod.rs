use std::collections::HashSet;

use zksync_contracts::BaseSystemContracts;
use zksync_test_account::{Account, TxType};
use zksync_types::{
    block::L2BlockHasher, utils::deployed_address_create, writes::StateDiffRecord, Address,
    L1BatchNumber, L2BlockNumber, Nonce, StorageKey, H256, U256,
};
use zksync_vm_interface::{
    CurrentExecutionState, VmExecutionResultAndLogs, VmInterfaceHistoryEnabled,
};

use super::{get_empty_storage, read_test_contract};
use crate::{
    interface::{
        storage::{InMemoryStorage, StoragePtr, StorageView},
        L1BatchEnv, L2Block, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode, VmFactory,
        VmInterfaceExt,
    },
    versions::testonly::{
        default_l1_batch, default_system_env, make_account_rich, ContractToDeploy,
    },
    vm_latest::utils::l2_blocks::load_last_l2_block,
};

//mod transaction_test_info; FIXME

// FIXME: revise fields
#[derive(Debug)]
pub(crate) struct VmTester<VM> {
    pub(crate) vm: VM,
    pub(crate) system_env: SystemEnv,
    pub(crate) l1_batch_env: L1BatchEnv,
    pub(crate) storage: StoragePtr<StorageView<InMemoryStorage>>,
    pub(crate) deployer: Option<Account>,
    pub(crate) test_contract: Option<Address>,
    pub(crate) rich_accounts: Vec<Account>,
    pub(crate) custom_contracts: Vec<ContractToDeploy>,
}

impl<VM> VmTester<VM>
where
    VM: VmFactory<StorageView<InMemoryStorage>>,
{
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
        self.storage = StorageView::new(get_empty_storage()).to_rc_ptr();
        self.reset_state(false);
    }

    /// Reset the state of the VM to the initial state.
    /// If `use_latest_l2_block` is true, then the VM will use the latest L2 block from storage,
    /// otherwise it will use the first L2 block of l1 batch env
    pub(crate) fn reset_state(&mut self, use_latest_l2_block: bool) {
        for account in self.rich_accounts.iter_mut() {
            account.nonce = Nonce(0);
            make_account_rich(self.storage.borrow_mut().inner_mut(), account);
        }
        if let Some(deployer) = &self.deployer {
            make_account_rich(self.storage.borrow_mut().inner_mut(), deployer);
        }

        if !self.custom_contracts.is_empty() {
            println!("Inserting custom contracts is not yet supported")
            // `insert_contracts(&mut self.storage, &self.custom_contracts);`
        }

        let l1_batch = &mut self.l1_batch_env;
        if use_latest_l2_block {
            let last_l2_block = load_last_l2_block(&self.storage).unwrap_or(L2Block {
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

        let vm = VM::new(
            l1_batch.clone(),
            self.system_env.clone(),
            self.storage.clone(),
        );
        if self.test_contract.is_some() {
            self.deploy_test_contract();
        }
        self.vm = vm;
    }
}

#[derive(Debug)]
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

    pub(crate) fn build<VM>(self) -> VmTester<VM>
    where
        VM: VmFactory<StorageView<InMemoryStorage>>,
    {
        let l1_batch_env = self
            .l1_batch_env
            .unwrap_or_else(|| default_l1_batch(L1BatchNumber(1)));

        let mut raw_storage = self.storage.unwrap_or_else(get_empty_storage);
        ContractToDeploy::insert_all(&self.custom_contracts, &mut raw_storage);
        let storage = StorageView::new(raw_storage).to_rc_ptr();
        for account in self.rich_accounts.iter() {
            make_account_rich(storage.borrow_mut().inner_mut(), account);
        }
        if let Some(deployer) = &self.deployer {
            make_account_rich(storage.borrow_mut().inner_mut(), deployer);
        }

        let vm = VM::new(
            l1_batch_env.clone(),
            self.system_env.clone(),
            storage.clone(),
        );
        VmTester {
            vm,
            system_env: self.system_env,
            l1_batch_env,
            storage,
            deployer: self.deployer,
            test_contract: None,
            rich_accounts: self.rich_accounts.clone(),
            custom_contracts: self.custom_contracts.clone(),
        }
    }
}

/// Test extensions for VM.
pub(crate) trait TestedVm:
    VmFactory<StorageView<InMemoryStorage>> + VmInterfaceHistoryEnabled
{
    fn gas_remaining(&mut self) -> u32;

    fn get_current_execution_state(&self) -> CurrentExecutionState;

    /// Unlike [`Self::known_bytecode_hashes()`], the output should only include successfully decommitted bytecodes.
    fn decommitted_hashes(&self) -> HashSet<U256>;

    fn execute_with_state_diffs(
        &mut self,
        diffs: Vec<StateDiffRecord>,
        mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs;

    fn insert_bytecodes(&mut self, bytecodes: &[&[u8]]);

    /// Includes bytecodes that have failed to decommit.
    fn known_bytecode_hashes(&self) -> HashSet<U256>;

    /// Returns `true` iff the decommit is fresh.
    fn manually_decommit(&mut self, code_hash: H256) -> bool;

    fn verify_required_bootloader_memory(&self, cells: &[(u32, U256)]);

    fn verify_required_storage(&mut self, cells: &[(StorageKey, H256)]);
}
