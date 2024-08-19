use std::{collections::HashMap, fmt, sync::Arc};

use once_cell::sync::OnceCell;
use zksync_types::{
    get_code_key, get_nonce_key, web3::keccak256, AccountTreeId, Address, StorageKey, StorageValue,
    H160, H256, L2_BASE_TOKEN_ADDRESS, U256,
};
use zksync_utils::{address_to_h256, h256_to_u256};

use crate::interface::storage::{StoragePtr, WriteStorage};

pub mod vm_1_4_1;
pub mod vm_1_4_2;
pub mod vm_latest;
pub mod vm_refunds_enhancement;
pub mod vm_virtual_blocks;

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
pub struct PrestateTracer {
    pub pre: State,
    pub post: State,
    pub config: PrestateTracerConfig,
    pub result: Arc<OnceCell<(State, State)>>,
}

impl PrestateTracer {
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

pub fn process_modified_storage_keys<S>(
    prestate: State,
    storage: &StoragePtr<S>,
) -> HashMap<H160, Account>
where
    S: WriteStorage,
{
    let cloned_storage = &storage.clone();
    let mut initial_storage_ref = cloned_storage.as_ref().borrow_mut();

    initial_storage_ref
        .modified_storage_keys()
        .clone()
        .iter()
        .filter(|k| !prestate.contains_key(k.0.account().address()))
        .map(|k| {
            (
                *(k.0.account().address()),
                Account {
                    balance: Some(h256_to_u256(
                        initial_storage_ref.read_value(&get_balance_key(k.0.account())),
                    )),
                    code: Some(h256_to_u256(
                        initial_storage_ref.read_value(&get_code_key(k.0.account().address())),
                    )),
                    nonce: Some(h256_to_u256(
                        initial_storage_ref.read_value(&get_nonce_key(k.0.account().address())),
                    )),
                    storage: Some(get_storage_if_present(
                        k.0.account(),
                        initial_storage_ref.modified_storage_keys(),
                    )),
                },
            )
        })
        .collect::<State>()
}

fn get_balance_key(account: &AccountTreeId) -> StorageKey {
    let address_h256 = address_to_h256(account.address());
    let bytes = [address_h256.as_bytes(), &[0; 32]].concat();
    let balance_key: H256 = keccak256(&bytes).into();
    StorageKey::new(AccountTreeId::new(L2_BASE_TOKEN_ADDRESS), balance_key)
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

fn get_account_data<T: StorageAccess>(
    account_key: &StorageKey,
    state: &T,
    storage: &HashMap<StorageKey, StorageValue>,
) -> (Address, Account) {
    let address = *(account_key.account().address());
    let balance = state.read_from_storage(&get_balance_key(account_key.account()));
    let code = state.read_from_storage(&get_code_key(account_key.account().address()));
    let nonce = state.read_from_storage(&get_nonce_key(account_key.account().address()));
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

// Define a trait that abstracts storage access
trait StorageAccess {
    fn read_from_storage(&self, key: &StorageKey) -> U256;
}
