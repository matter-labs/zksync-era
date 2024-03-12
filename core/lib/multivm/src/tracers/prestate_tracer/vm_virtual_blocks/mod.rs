use std::{collections::HashMap, sync::Arc};

use once_cell::sync::OnceCell;
use zk_evm_1_3_3::tracing::{BeforeExecutionData, VmLocalStateData};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{
    get_code_key, get_nonce_key, web3::signing::keccak256, AccountTreeId, Address, StorageKey,
    StorageValue, H256, L2_ETH_TOKEN_ADDRESS,
};
use zksync_utils::address_to_h256;

use super::{process_modified_storage_keys, Account, PrestateTracer, State};
use crate::{
    interface::dyn_tracers::vm_1_3_3::DynTracer,
    vm_virtual_blocks::{
        BootloaderState, ExecutionEndTracer, ExecutionProcessing, HistoryMode, SimpleMemory,
        ZkSyncVmState,
    },
};

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for PrestateTracer {
    fn before_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        storage: StoragePtr<S>,
    ) {
        if self.config.diff_mode {
            self.pre
                .extend(process_modified_storage_keys(self.pre.clone(), &storage));
        }
    }
}

impl<H: HistoryMode> ExecutionEndTracer<H> for PrestateTracer {}

impl<S: WriteStorage, H: HistoryMode> ExecutionProcessing<S, H> for PrestateTracer {
    fn after_vm_execution(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: crate::interface::tracer::VmExecutionStopReason,
    ) {
        let modified_storage_keys = state.storage.storage.inner().get_modified_storage_keys();
        if self.config.diff_mode {
            self.post = modified_storage_keys
                .clone()
                .keys()
                .copied()
                .collect::<Vec<_>>()
                .iter()
                .map(|k| get_account_data(k, state, &modified_storage_keys))
                .collect::<State>();
        } else {
            let read_keys = &state.storage.read_keys;
            let map = read_keys.inner().clone();
            let res = map
                .keys()
                .copied()
                .collect::<Vec<_>>()
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

fn get_account_data<S: zksync_state::WriteStorage, H: HistoryMode>(
    account_key: &StorageKey,
    state: &ZkSyncVmState<S, H>,
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
