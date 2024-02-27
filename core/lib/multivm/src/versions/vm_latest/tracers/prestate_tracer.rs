use std::fmt;
use std::{collections::HashMap, marker::PhantomData};

use crate::{
    interface::{
        dyn_tracers::vm_1_4_1::DynTracer,
        tracer::{TracerExecutionStatus, TracerExecutionStopReason},
        types::inputs::L1BatchEnv,
        VmExecutionMode,
    },
    vm_latest::{
        bootloader_state::{utils::apply_pubdata_to_memory, BootloaderState},
        constants::BOOTLOADER_HEAP_PAGE,
        old_vm::{history_recorder::HistoryMode, memory::SimpleMemory},
        tracers::{traits::VmTracer, utils::VmHook},
        types::internals::{PubdataInput, ZkSyncVmState},
        utils::logs::collect_events_and_l1_system_logs_after_timestamp,
        StorageOracle,
    },
};
use zk_evm_1_4_1::{
    aux_structures::Timestamp,
    tracing::{BeforeExecutionData, VmLocalStateData},
};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{
    get_code_key, get_nonce_key, web3::signing::keccak256, AccountTreeId, Address, Bytes,
    StorageKey, StorageValue, H160, H256, L2_ETH_TOKEN_ADDRESS, U256,
};
use zksync_utils::address_to_h256;

#[derive(Debug, Clone)]
pub struct Account {
    balance: Option<U256>,
    code: Option<U256>,
    nonce: Option<U256>,
    storage: Option<HashMap<H256, H256>>,
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
}

impl PrestateTracer {
    pub(crate) fn new() -> Self {
        println!("NEW");
        Self {
            pre: Default::default(),
            post: Default::default(),
            config: PrestateTracerConfig { diff_mode: false },
        }
    }

    pub(crate) fn get_pre(&self) -> &State {
        &self.pre
    }
}

#[derive(Debug, Clone)]
pub struct PrestateTracerConfig {
    diff_mode: bool, // If true, this tracer will return state modifications
}

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for PrestateTracer {
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
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
        let modified_storage_keys = {
            let binding = state.storage.storage.inner().storage_ptr.borrow_mut();
            // Clone the keys or derive the needed information here
            binding.modified_storage_keys().clone() // Assuming `clone` is possible and inexpensive
        };

        let read_keys = &state.storage.read_keys;
        let map = read_keys.inner().clone();
        let storage_keys_read = map.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
        let pre = storage_keys_read
            .iter()
            .map(|k| {
                let nonce_key = get_nonce_key(k.account().address());
                let code_key = get_code_key(k.account().address());
                let nonce = state.storage.storage.read_from_storage(&nonce_key);
                let address_h256 = address_to_h256(k.account().address());
                let bytes = [address_h256.as_bytes(), &[0; 32]].concat();
                let balance_key: H256 = keccak256(&bytes).into();
                let balance_storage_key =
                    StorageKey::new(AccountTreeId::new(L2_ETH_TOKEN_ADDRESS), balance_key);
                let balance = state
                    .storage
                    .storage
                    .read_from_storage(&balance_storage_key);

                let code = state.storage.storage.read_from_storage(&code_key);
                let storage = get_storage_if_present(k.account(), &modified_storage_keys);
                (
                    *(k.account().address()),
                    Account {
                        balance: Some(balance),
                        code: Some(code),
                        nonce: Some(nonce),
                        storage: Some(storage),
                    },
                )
            })
            .collect::<State>();
        self.pre = pre;
    }
}

fn get_storage_if_present(
    account: &AccountTreeId,
    modified_storage_keys: &HashMap<StorageKey, StorageValue>,
) -> HashMap<H256, H256> {
    //check if there is a StorageKey struct wioth an account field that matches the account and return the key as the key and the StorageValue as the value
    modified_storage_keys
        .iter()
        .filter(|(k, _)| k.account() == account)
        .map(|(k, v)| (k.key().clone(), v.clone()))
        .collect()
}
