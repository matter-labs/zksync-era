mod types;
mod vm_latest;
mod vm_refunds_enhancement;
mod vm_virtual_blocks;

use std::sync::Arc;
use std::{collections::HashSet, marker::PhantomData};

use once_cell::sync::OnceCell;

use zksync_state::{StoragePtr, WriteStorage};
use zksync_system_constants::{
    ACCOUNT_CODE_STORAGE_ADDRESS, BOOTLOADER_ADDRESS, CONTRACT_DEPLOYER_ADDRESS,
    L2_ETH_TOKEN_ADDRESS, MSG_VALUE_SIMULATOR_ADDRESS, SYSTEM_CONTEXT_ADDRESS,
};

use zksync_types::{
    vm_trace::ViolatedValidationRule, web3::signing::keccak256, AccountTreeId, Address, StorageKey,
    H256, U256,
};
use zksync_utils::{be_bytes_to_safe_address, u256_to_account_address, u256_to_h256};

use crate::tracers::validator::types::{NewTrustedValidationItems, ValidationTracerMode};
pub use crate::tracers::validator::types::{ValidationError, ValidationTracerParams};

/// Tracer that is used to ensure that the validation adheres to all the rules
/// to prevent DDoS attacks on the server.
#[derive(Debug, Clone)]
pub struct ValidationTracer<H> {
    validation_mode: ValidationTracerMode,
    auxilary_allowed_slots: HashSet<H256>,

    user_address: Address,
    #[allow(dead_code)]
    paymaster_address: Address,
    should_stop_execution: bool,
    trusted_slots: HashSet<(Address, U256)>,
    trusted_addresses: HashSet<Address>,
    trusted_address_slots: HashSet<(Address, U256)>,
    computational_gas_used: u32,
    computational_gas_limit: u32,
    pub result: Arc<OnceCell<ViolatedValidationRule>>,
    _marker: PhantomData<fn(H) -> H>,
}

type ValidationRoundResult = Result<NewTrustedValidationItems, ViolatedValidationRule>;

impl<H> ValidationTracer<H> {
    pub fn new(params: ValidationTracerParams) -> (Self, Arc<OnceCell<ViolatedValidationRule>>) {
        let result = Arc::new(OnceCell::new());
        (
            Self {
                validation_mode: ValidationTracerMode::NoValidation,
                auxilary_allowed_slots: Default::default(),

                should_stop_execution: false,
                user_address: params.user_address,
                paymaster_address: params.paymaster_address,
                trusted_slots: params.trusted_slots,
                trusted_addresses: params.trusted_addresses,
                trusted_address_slots: params.trusted_address_slots,
                computational_gas_used: 0,
                computational_gas_limit: params.computational_gas_limit,
                result: result.clone(),
                _marker: Default::default(),
            },
            result,
        )
    }

    fn process_validation_round_result(&mut self, result: ValidationRoundResult) {
        match result {
            Ok(NewTrustedValidationItems {
                new_allowed_slots,
                new_trusted_addresses,
            }) => {
                self.auxilary_allowed_slots.extend(new_allowed_slots);
                self.trusted_addresses.extend(new_trusted_addresses);
            }
            Err(err) => {
                if self.result.get().is_some() {
                    tracing::trace!("Validation error is already set, skipping");
                    return;
                }
                self.result.set(err).expect("Result should be empty");
            }
        }
    }

    // Checks whether such storage access is acceptable.
    fn is_allowed_storage_read<S: WriteStorage>(
        &self,
        storage: StoragePtr<S>,
        address: Address,
        key: U256,
        msg_sender: Address,
    ) -> bool {
        // If there are no restrictions, all storage reads are valid.
        // We also don't support the paymaster validation for now.
        if matches!(
            self.validation_mode,
            ValidationTracerMode::NoValidation | ValidationTracerMode::PaymasterTxValidation
        ) {
            return true;
        }

        // The pair of MSG_VALUE_SIMULATOR_ADDRESS & L2_ETH_TOKEN_ADDRESS simulates the behavior of transfering ETH
        // that is safe for the DDoS protection rules.
        if valid_eth_token_call(address, msg_sender) {
            return true;
        }

        if self.trusted_slots.contains(&(address, key))
            || self.trusted_addresses.contains(&address)
            || self.trusted_address_slots.contains(&(address, key))
        {
            return true;
        }

        if touches_allowed_context(address, key) {
            return true;
        }

        // The user is allowed to touch its own slots or slots semantically related to him.
        let valid_users_slot = address == self.user_address
            || u256_to_account_address(&key) == self.user_address
            || self.auxilary_allowed_slots.contains(&u256_to_h256(key));
        if valid_users_slot {
            return true;
        }

        if is_constant_code_hash(address, key, storage) {
            return true;
        }

        false
    }

    // Used to remember user-related fields (its balance/allowance/etc).
    // Note that it assumes that the length of the calldata is 64 bytes.
    fn slot_to_add_from_keccak_call(
        &self,
        calldata: &[u8],
        validated_address: Address,
    ) -> Option<H256> {
        assert_eq!(calldata.len(), 64);

        let (potential_address_bytes, potential_position_bytes) = calldata.split_at(32);
        let potential_address = be_bytes_to_safe_address(potential_address_bytes);

        // If the validation_address is equal to the potential_address,
        // then it is a request that could be used for mapping of kind mapping(address => ...).
        //
        // If the potential_position_bytes were already allowed before, then this keccak might be used
        // for ERC-20 allowance or any other of mapping(address => mapping(...))
        if potential_address == Some(validated_address)
            || self
                .auxilary_allowed_slots
                .contains(&H256::from_slice(potential_position_bytes))
        {
            // This is request that could be used for mapping of kind mapping(address => ...)

            // We could theoretically wait for the slot number to be returned by the
            // keccak256 precompile itself, but this would complicate the code even further
            // so let's calculate it here.
            let slot = keccak256(calldata);

            // Adding this slot to the allowed ones
            Some(H256(slot))
        } else {
            None
        }
    }

    pub fn params(&self) -> ValidationTracerParams {
        ValidationTracerParams {
            user_address: self.user_address,
            paymaster_address: self.paymaster_address,
            trusted_slots: self.trusted_slots.clone(),
            trusted_addresses: self.trusted_addresses.clone(),
            trusted_address_slots: self.trusted_address_slots.clone(),
            computational_gas_limit: self.computational_gas_limit,
        }
    }
}

fn touches_allowed_context(address: Address, key: U256) -> bool {
    // Context is not touched at all
    if address != SYSTEM_CONTEXT_ADDRESS {
        return false;
    }

    // Only chain_id is allowed to be touched.
    key == U256::from(0u32)
}

fn is_constant_code_hash<S: WriteStorage>(
    address: Address,
    key: U256,
    storage: StoragePtr<S>,
) -> bool {
    if address != ACCOUNT_CODE_STORAGE_ADDRESS {
        // Not a code hash
        return false;
    }

    let value = storage.borrow_mut().read_value(&StorageKey::new(
        AccountTreeId::new(address),
        u256_to_h256(key),
    ));

    value != H256::zero()
}

fn valid_eth_token_call(address: Address, msg_sender: Address) -> bool {
    let is_valid_caller = msg_sender == MSG_VALUE_SIMULATOR_ADDRESS
        || msg_sender == CONTRACT_DEPLOYER_ADDRESS
        || msg_sender == BOOTLOADER_ADDRESS;
    address == L2_ETH_TOKEN_ADDRESS && is_valid_caller
}
