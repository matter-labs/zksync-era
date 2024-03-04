use std::{collections::HashMap, fmt, rc::Rc, sync::Arc};

use once_cell::sync::OnceCell;
use zk_evm_1_4_1::tracing::{BeforeExecutionData, VmLocalStateData};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{
    get_code_key, get_nonce_key, web3::signing::keccak256, AccountTreeId, Address, StorageKey,
    StorageValue, H256, L2_ETH_TOKEN_ADDRESS, U256,
};
use zksync_utils::{address_to_h256, h256_to_u256};

use crate::{
    interface::{dyn_tracers::vm_1_4_1::DynTracer, tracer::TracerExecutionStatus},
    vm_latest::{
        self,
        bootloader_state::BootloaderState,
        old_vm::{history_recorder::HistoryMode, memory::SimpleMemory},
        tracers::traits::VmTracer,
        types::internals::ZkSyncVmState,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub balance: Option<U256>,
    pub code: Option<U256>,
    pub nonce: Option<U256>,
    pub storage: Option<HashMap<H256, H256>>,
}

impl fmt::Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{{")?;
        if let Some(balance) = self.balance {
            writeln!(f, "  balance: \"0x{:x}\",", balance)?;
        }
        if let Some(code) = &self.code {
            writeln!(f, "  code: \"{}\",", code)?;
        }
        if let Some(nonce) = self.nonce {
            writeln!(f, "  nonce: {},", nonce)?;
        }
        if let Some(storage) = &self.storage {
            writeln!(f, "  storage: {{")?;
            for (key, value) in storage.iter() {
                writeln!(f, "    {}: \"{}\",", key, value)?;
            }
            writeln!(f, "  }}")?;
        }
        writeln!(f, "}}")
    }
}

type State = HashMap<Address, Account>;

#[derive(Debug, Clone)]
pub(crate) struct PrestateTracer {
    pub pre: State,
    pub post: State,
    pub config: PrestateTracerConfig,
    pub result: Arc<OnceCell<(State, State)>>,
}

impl PrestateTracer {
    #[allow(dead_code)]
    pub fn new(diff_mode: bool, result: Arc<OnceCell<(State, State)>>) -> Self {
        Self {
            pre: Default::default(),
            post: Default::default(),
            config: PrestateTracerConfig { diff_mode },
            result,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrestateTracerConfig {
    diff_mode: bool,
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for PrestateTracer {
    fn before_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        storage: StoragePtr<S>,
    ) {
        if self.config.diff_mode && self.pre.is_empty() {
            let cloned_storage = Rc::clone(&storage);
            let mut initial_storage_ref = cloned_storage.as_ref().borrow_mut();
            let keys = initial_storage_ref
                .modified_storage_keys()
                .keys()
                .cloned()
                .collect::<Vec<_>>();

            let res = keys
                .iter()
                .map(|k| {
                    (
                        *(k.account().address()),
                        Account {
                            balance: Some(h256_to_u256(
                                initial_storage_ref.read_value(&get_balance_key(k.account())),
                            )),
                            code: Some(h256_to_u256(
                                initial_storage_ref
                                    .read_value(&get_code_key(k.account().address())),
                            )),
                            nonce: Some(h256_to_u256(
                                initial_storage_ref
                                    .read_value(&get_nonce_key(k.account().address())),
                            )),
                            storage: Some(get_storage_if_present(
                                k.account(),
                                &initial_storage_ref.modified_storage_keys(),
                            )),
                        },
                    )
                })
                .collect::<State>();
            self.pre = res;
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for PrestateTracer {
    fn initialize_tracer(&mut self, _state: &mut ZkSyncVmState<S, H>) {}

    fn finish_cycle(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        TracerExecutionStatus::Continue
    }

    fn after_vm_execution(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: crate::interface::tracer::VmExecutionStopReason,
    ) {
        let modified_storage_keys = state.storage.storage.inner().get_modified_storage_keys();
        if self.config.diff_mode {
            let diff = modified_storage_keys
                .clone()
                .keys()
                .copied()
                .collect::<Vec<_>>();

            let res = diff
                .iter()
                .map(|k| {
                    (
                        *(k.account().address()),
                        Account {
                            balance: Some(
                                state
                                    .storage
                                    .storage
                                    .read_from_storage(&get_balance_key(k.account())),
                            ),
                            code: Some(
                                state
                                    .storage
                                    .storage
                                    .read_from_storage(&get_code_key(k.account().address())),
                            ),
                            nonce: Some(
                                state
                                    .storage
                                    .storage
                                    .read_from_storage(&get_nonce_key(k.account().address())),
                            ),
                            storage: Some(get_storage_if_present(
                                k.account(),
                                &modified_storage_keys,
                            )),
                        },
                    )
                })
                .collect::<State>();
            self.post = res;
        } else {
            let read_keys = &state.storage.read_keys;
            let map = read_keys.inner().clone();
            let storage_keys_read = map.keys().copied().collect::<Vec<_>>();
            let res = storage_keys_read
                .iter()
                .map(|k| get_account_data(k, state, &modified_storage_keys))
                .collect::<State>();
            self.post = res;
        }
        process_result(&self.result, self.pre.clone(), self.post.clone());
    }
}

fn get_balance_key(account: &AccountTreeId) -> StorageKey {
    let address_h256 = address_to_h256(account.address());
    let bytes = [address_h256.as_bytes(), &[0; 32]].concat();
    let balance_key: H256 = keccak256(&bytes).into();
    StorageKey::new(AccountTreeId::new(L2_ETH_TOKEN_ADDRESS), balance_key)
}

fn get_storage_if_present(
    account: &AccountTreeId,
    modified_storage_keys: &HashMap<StorageKey, StorageValue>,
) -> HashMap<H256, H256> {
    //check if there is a Storage Key struct with an account field that matches the account and return the key as the key and the Storage Value as the value
    modified_storage_keys
        .iter()
        .filter(|(k, _)| k.account() == account)
        .map(|(k, v)| (*k.key(), *v))
        .collect()
}

fn process_result(result: &Arc<OnceCell<(State, State)>>, mut pre: State, post: State) {
    pre.retain(|k, v| {
        if let Some(post_v) = post.get(k) {
            if v != post_v {
                return true;
            }
        }
        false
    });
    result.set((pre, post)).unwrap();
}

fn get_account_data<
    S: zksync_state::WriteStorage,
    H: vm_latest::old_vm::history_recorder::HistoryMode,
>(
    account_key: &StorageKey,
    state: &mut ZkSyncVmState<S, H>,
    storage: &HashMap<StorageKey, StorageValue>,
) -> (Address, Account) {
    let address = *(account_key.account().address());
    let balance = state
        .storage
        .storage
        .read_from_storage(&get_balance_key(account_key.account()));
    let code = state
        .storage
        .storage
        .read_from_storage(&get_code_key(account_key.account().address()));
    let nonce = state
        .storage
        .storage
        .read_from_storage(&get_nonce_key(account_key.account().address()));
    let storage = get_storage_if_present(account_key.account(), storage);

    (
        address,
        Account {
            balance: Some(balance),
            code: Some(code),
            nonce: Some(nonce),
            storage: Some(storage),
        },
    )
}
