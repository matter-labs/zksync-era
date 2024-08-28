use std::marker::PhantomData;

use zksync_contracts::{l2_rollup_da_validator_bytecode, BaseSystemContracts};
use zksync_types::{
    block::L2BlockHasher,
    commitment::{L1BatchCommitmentMode, PubdataParams},
    fee_model::BatchFeeInput,
    get_code_key, get_is_account_key,
    helpers::unix_timestamp_ms,
    utils::{deployed_address_create, storage_key_for_eth_balance},
    Address, L1BatchNumber, L2BlockNumber, L2ChainId, Nonce, ProtocolVersionId, U256,
};
use zksync_utils::{bytecode::hash_bytecode, u256_to_h256};

use crate::{
    interface::{
        storage::{InMemoryStorage, StoragePtr, StorageView, WriteStorage},
        L1BatchEnv, L2Block, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode, VmFactory,
        VmInterface,
    },
    vm_latest::{
        constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
        tests::{
            tester::{Account, TxType},
            utils::read_test_contract,
        },
        utils::l2_blocks::load_last_l2_block,
        Vm,
    },
    HistoryMode,
};

pub(crate) type InMemoryStorageView = StorageView<InMemoryStorage>;

pub(crate) struct VmTester<H: HistoryMode> {
    pub(crate) vm: Vm<InMemoryStorageView, H>,
    pub(crate) storage: StoragePtr<InMemoryStorageView>,
    pub(crate) fee_account: Address,
    pub(crate) deployer: Option<Account>,
    pub(crate) test_contract: Option<Address>,
    pub(crate) rich_accounts: Vec<Account>,
    pub(crate) custom_contracts: Vec<ContractsToDeploy>,
    _phantom: std::marker::PhantomData<H>,
}

impl<H: HistoryMode> VmTester<H> {
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

    // pub(crate) fn reset_with_empty_storage(&mut self) {
    //     self.storage = StorageView::new(get_empty_storage()).to_rc_ptr();
    //     self.reset_state(false);
    // }

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

        let mut l1_batch = self.vm.batch_env.clone();
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

        let vm = Vm::new(l1_batch, self.vm.system_env.clone(), self.storage.clone());

        if self.test_contract.is_some() {
            self.deploy_test_contract();
        }

        self.vm = vm;
    }
}

pub(crate) type ContractsToDeploy = (Vec<u8>, Address, bool);

pub(crate) struct VmTesterBuilder<H: HistoryMode> {
    storage: Option<InMemoryStorage>,
    l1_batch_env: Option<L1BatchEnv>,
    system_env: SystemEnv,
    deployer: Option<Account>,
    rich_accounts: Vec<Account>,
    custom_contracts: Vec<ContractsToDeploy>,
    pubdata_params: Option<PubdataParams>,
    _phantom: PhantomData<H>,
}

impl<H: HistoryMode> Clone for VmTesterBuilder<H> {
    fn clone(&self) -> Self {
        Self {
            storage: None,
            l1_batch_env: self.l1_batch_env.clone(),
            system_env: self.system_env.clone(),
            deployer: self.deployer.clone(),
            rich_accounts: self.rich_accounts.clone(),
            custom_contracts: self.custom_contracts.clone(),
            pubdata_params: self.pubdata_params,
            _phantom: PhantomData,
        }
    }
}

#[allow(dead_code)]
impl<H: HistoryMode> VmTesterBuilder<H> {
    pub(crate) fn new(_: H) -> Self {
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
            _phantom: PhantomData,
            pubdata_params: None,
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

    pub(crate) fn with_custom_pubdata_params(mut self, pubdata_params: PubdataParams) -> Self {
        self.pubdata_params = Some(pubdata_params);
        self
    }

    pub(crate) fn with_rollup_pubdata_params(mut self, fixed_address: Option<Address>) -> Self {
        // We choose some random address to put the L2 DA validator to.
        let l2_da_validator_address = fixed_address.unwrap_or_else(Address::random);

        let bytecode = l2_rollup_da_validator_bytecode();

        self.pubdata_params = Some(PubdataParams {
            l2_da_validator_address,
            pubdata_type: L1BatchCommitmentMode::Rollup,
        });

        self.custom_contracts
            .push((bytecode, l2_da_validator_address, false));

        self
    }

    pub(crate) fn with_validium_pubdata_params(self) -> Self {
        todo!()
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

    pub(crate) fn build(mut self) -> VmTester<H> {
        if self.pubdata_params.is_none() {
            self = self.with_rollup_pubdata_params(None);
        }
        self.system_env.pubdata_params = self.pubdata_params.unwrap();

        let l1_batch_env = self
            .l1_batch_env
            .unwrap_or_else(|| default_l1_batch(L1BatchNumber(1)));

        let mut raw_storage = self.storage.unwrap_or_else(get_empty_storage);
        insert_contracts(&mut raw_storage, &self.custom_contracts);
        let storage_ptr = StorageView::new(raw_storage).to_rc_ptr();
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
            fee_account,
            deployer: self.deployer,
            test_contract: None,
            rich_accounts: self.rich_accounts.clone(),
            custom_contracts: self.custom_contracts.clone(),
            _phantom: PhantomData,
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

pub(crate) fn make_account_rich(storage: StoragePtr<InMemoryStorageView>, account: &Account) {
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
