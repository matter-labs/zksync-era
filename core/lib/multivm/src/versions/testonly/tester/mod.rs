use std::{collections::HashSet, fmt, rc::Rc};

use zksync_contracts::BaseSystemContracts;
use zksync_test_contracts::{Account, TestContract, TxType};
use zksync_types::{
    get_deployer_key,
    l2::L2Tx,
    system_contracts::get_system_smart_contracts,
    utils::{deployed_address_create, storage_key_for_eth_balance},
    writes::StateDiffRecord,
    Address, L1BatchNumber, L2ChainId, StorageKey, Transaction, H256, U256,
};
use zksync_vm_interface::Call;

pub(crate) use self::transaction_test_info::{ExpectedError, TransactionTestInfo, TxModifier};
use super::get_empty_storage;
use crate::{
    interface::{
        pubdata::{PubdataBuilder, PubdataInput},
        storage::{InMemoryStorage, StoragePtr, StorageView},
        tracer::{ValidationParams, ViolatedValidationRule},
        CurrentExecutionState, InspectExecutionMode, L1BatchEnv, L2BlockEnv, SystemEnv,
        TxExecutionMode, VmExecutionResultAndLogs, VmFactory, VmInterfaceExt,
        VmInterfaceHistoryEnabled,
    },
    versions::testonly::{
        default_l1_batch, default_system_env, make_address_rich, ContractToDeploy,
    },
};

mod transaction_test_info;

/// VM tester that provides prefunded accounts, storage handle etc.
#[derive(Debug)]
pub(crate) struct VmTester<VM> {
    pub(crate) vm: VM,
    pub(crate) system_env: SystemEnv,
    pub(crate) l1_batch_env: L1BatchEnv,
    pub(crate) storage: StoragePtr<StorageView<InMemoryStorage>>,
    pub(crate) test_contract: Option<Address>,
    pub(crate) rich_accounts: Vec<Account>,
}

impl<VM: TestedVm> VmTester<VM> {
    pub(crate) fn deploy_test_contract(&mut self) {
        let contract = TestContract::counter().bytecode;
        let account = &mut self.rich_accounts[0];
        let tx = account.get_deploy_tx(contract, None, TxType::L2).tx;
        let nonce = tx.nonce().unwrap().0.into();
        self.vm.push_transaction(tx);
        self.vm.execute(InspectExecutionMode::OneTx);
        let deployed_address = deployed_address_create(account.address, nonce);
        self.test_contract = Some(deployed_address);
    }

    pub(crate) fn get_eth_balance(&mut self, address: Address) -> U256 {
        self.vm.read_storage(storage_key_for_eth_balance(&address))
    }

    pub(crate) fn reset_with_empty_storage(&mut self) {
        let mut storage = get_empty_storage();
        for account in &self.rich_accounts {
            make_address_rich(&mut storage, account.address);
        }

        let storage = StorageView::new(storage).to_rc_ptr();
        self.storage = storage.clone();
        self.vm = VM::new(self.l1_batch_env.clone(), self.system_env.clone(), storage);
    }
}

/// Builder for [`VmTester`].
#[derive(Debug)]
pub(crate) struct VmTesterBuilder {
    storage: Option<InMemoryStorage>,
    storage_slots: Vec<(StorageKey, H256)>,
    l1_batch_env: Option<L1BatchEnv>,
    system_env: SystemEnv,
    rich_accounts: Vec<Account>,
    custom_contracts: Vec<ContractToDeploy>,
    custom_evm_contracts: Vec<ContractToDeploy>,
    enable_evm_emulator: bool,
}

impl VmTesterBuilder {
    pub(crate) fn new() -> Self {
        Self {
            storage: None,
            storage_slots: vec![],
            l1_batch_env: None,
            system_env: default_system_env(),
            rich_accounts: vec![],
            custom_contracts: vec![],
            custom_evm_contracts: vec![],
            enable_evm_emulator: false,
        }
    }

    pub(crate) fn with_system_env(mut self, system_env: SystemEnv) -> Self {
        self.system_env = system_env;
        self
    }

    pub(crate) fn with_l1_batch_env(mut self, l1_batch_env: L1BatchEnv) -> Self {
        self.l1_batch_env = Some(l1_batch_env);
        self
    }

    pub(crate) fn with_storage(mut self, storage: InMemoryStorage) -> Self {
        self.storage = Some(storage);
        self
    }

    pub(crate) fn with_storage_slots(
        mut self,
        slots: impl IntoIterator<Item = (StorageKey, H256)>,
    ) -> Self {
        self.storage_slots = slots.into_iter().collect();
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

    /// Creates the specified number of pre-funded accounts.
    pub(crate) fn with_rich_accounts(mut self, number: u32) -> Self {
        for i in 0..number {
            self.rich_accounts.push(Account::from_seed(i));
        }
        self
    }

    pub(crate) fn rich_account(&self, index: usize) -> &Account {
        &self.rich_accounts[index]
    }

    pub(crate) fn with_custom_contracts(mut self, contracts: Vec<ContractToDeploy>) -> Self {
        self.custom_contracts = contracts;
        self
    }

    pub(crate) fn with_evm_contracts(mut self, contracts: Vec<ContractToDeploy>) -> Self {
        self.custom_evm_contracts = contracts;
        self
    }

    pub(crate) fn with_evm_emulator(mut self) -> Self {
        self.enable_evm_emulator = true;
        self
    }

    pub(crate) fn build<VM>(self) -> VmTester<VM>
    where
        VM: VmFactory<StorageView<InMemoryStorage>>,
    {
        let enable_evm_emulator = self.enable_evm_emulator || !self.custom_evm_contracts.is_empty();
        let system_env = self.system_env;
        if enable_evm_emulator {
            assert!(system_env
                .base_system_smart_contracts
                .evm_emulator
                .is_some());
        }

        let l1_batch_env = self
            .l1_batch_env
            .unwrap_or_else(|| default_l1_batch(L1BatchNumber(1)));

        let mut raw_storage = self.storage.unwrap_or_else(|| {
            InMemoryStorage::with_custom_system_contracts_and_chain_id(
                L2ChainId::default(),
                get_system_smart_contracts(),
            )
        });
        for (key, value) in self.storage_slots {
            raw_storage.set_value(key, value);
        }
        if enable_evm_emulator {
            // Set `ALLOWED_BYTECODE_TYPES_MODE_SLOT` in `ContractDeployer`.
            raw_storage.set_value(
                get_deployer_key(H256::from_low_u64_be(1)),
                H256::from_low_u64_be(1),
            );
        }

        for contract in self.custom_contracts {
            contract.insert(&mut raw_storage);
        }
        for contract in self.custom_evm_contracts {
            contract.insert_evm(&mut raw_storage);
        }

        let storage = StorageView::new(raw_storage).to_rc_ptr();
        for account in &self.rich_accounts {
            make_address_rich(storage.borrow_mut().inner_mut(), account.address);
        }

        let vm = VM::new(l1_batch_env.clone(), system_env.clone(), storage.clone());
        VmTester {
            vm,
            system_env,
            l1_batch_env,
            storage,
            test_contract: None,
            rich_accounts: self.rich_accounts.clone(),
        }
    }
}

/// Test extensions for VM.
pub(crate) trait TestedVm:
    VmFactory<StorageView<InMemoryStorage>> + VmInterfaceHistoryEnabled
{
    type StateDump: fmt::Debug + PartialEq;

    fn dump_state(&self) -> Self::StateDump;

    fn gas_remaining(&mut self) -> u32;

    fn get_current_execution_state(&self) -> CurrentExecutionState;

    /// Unlike [`Self::known_bytecode_hashes()`], the output should only include successfully decommitted bytecodes.
    fn decommitted_hashes(&self) -> HashSet<U256>;

    fn finish_batch_with_state_diffs(
        &mut self,
        diffs: Vec<StateDiffRecord>,
        pubdata_builder: Rc<dyn PubdataBuilder>,
    ) -> VmExecutionResultAndLogs;

    fn finish_batch_without_pubdata(&mut self) -> VmExecutionResultAndLogs;

    fn insert_bytecodes(&mut self, bytecodes: &[&[u8]]);

    /// Includes bytecodes that have failed to decommit. Should exclude base system contract bytecodes (default AA / EVM emulator).
    fn known_bytecode_hashes(&self) -> HashSet<U256>;

    /// Returns `true` iff the decommit is fresh.
    fn manually_decommit(&mut self, code_hash: H256) -> bool;

    fn verify_required_bootloader_heap(&self, cells: &[(u32, U256)]);

    fn write_to_bootloader_heap(&mut self, cells: &[(usize, U256)]);

    /// Reads storage accounting for changes made during the VM run.
    fn read_storage(&mut self, key: StorageKey) -> U256;

    fn verify_required_storage(&mut self, cells: &[(StorageKey, U256)]) {
        for &(key, expected_value) in cells {
            assert_eq!(
                self.read_storage(key),
                expected_value,
                "Unexpected storage value at {key:?}"
            );
        }
    }

    /// Returns the current hash of the latest L2 block.
    fn last_l2_block_hash(&self) -> H256;

    /// Same as `start_new_l2_block`, but should skip consistency checks (to verify they are performed by the bootloader).
    fn push_l2_block_unchecked(&mut self, block: L2BlockEnv);

    /// Pushes a transaction with predefined refund value.
    fn push_transaction_with_refund(&mut self, tx: Transaction, refund: u64);

    /// Returns pubdata input.
    fn pubdata_input(&self) -> PubdataInput;
}

pub(crate) trait TestedVmForValidation: TestedVm {
    fn run_validation(
        &mut self,
        tx: L2Tx,
        timestamp: u64,
    ) -> (VmExecutionResultAndLogs, Option<ViolatedValidationRule>);
}

pub(crate) fn validation_params(tx: &L2Tx, system: &SystemEnv) -> ValidationParams {
    let user_address = tx.common_data.initiator_address;
    let paymaster_address = tx.common_data.paymaster_params.paymaster;
    ValidationParams {
        user_address,
        paymaster_address,
        trusted_slots: Default::default(),
        trusted_addresses: Default::default(),
        // field `trustedAddress` of ValidationRuleBreaker
        trusted_address_slots: [(Address::repeat_byte(0x10), 1.into())].into(),
        computational_gas_limit: system.default_validation_computational_gas_limit,
        timestamp_asserter_params: None,
    }
}

pub(crate) trait TestedVmWithCallTracer: TestedVm {
    fn inspect_with_call_tracer(&mut self) -> (VmExecutionResultAndLogs, Vec<Call>);
}

pub(crate) trait TestedVmWithStorageLimit: TestedVm {
    fn execute_with_storage_limit(&mut self, limit: usize) -> VmExecutionResultAndLogs;
}
