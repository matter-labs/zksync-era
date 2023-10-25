use std::{collections::HashSet, fmt::Display, marker::PhantomData};

use crate::vm_1_3_2::{
    errors::VmRevertReasonParsingResult,
    history_recorder::HistoryMode,
    memory::SimpleMemory,
    oracles::tracer::{
        utils::{computational_gas_price, print_debug_if_needed, VmHook},
        ExecutionEndTracer, PendingRefundTracer, PubdataSpentTracer,
    },
};

use zk_evm_1_3_3::{
    tracing::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    zkevm_opcode_defs::{ContextOpcode, FarCallABI, LogOpcode, Opcode},
};
use zksync_state::{StoragePtr, WriteStorage};

use crate::vm_1_3_2::oracles::tracer::{utils::get_calldata_page_via_abi, StorageInvocationTracer};
use zksync_system_constants::{
    ACCOUNT_CODE_STORAGE_ADDRESS, BOOTLOADER_ADDRESS, CONTRACT_DEPLOYER_ADDRESS,
    KECCAK256_PRECOMPILE_ADDRESS, L2_ETH_TOKEN_ADDRESS, MSG_VALUE_SIMULATOR_ADDRESS,
    SYSTEM_CONTEXT_ADDRESS,
};
use zksync_types::{
    get_code_key, web3::signing::keccak256, AccountTreeId, Address, StorageKey, H256, U256,
};
use zksync_utils::{
    be_bytes_to_safe_address, h256_to_account_address, u256_to_account_address, u256_to_h256,
};

#[derive(Debug, Clone, Eq, PartialEq, Copy)]
#[allow(clippy::enum_variant_names)]
pub enum ValidationTracerMode {
    // Should be activated when the transaction is being validated by user.
    UserTxValidation,
    // Should be activated when the transaction is being validated by the paymaster.
    PaymasterTxValidation,
    // Is a state when there are no restrictions on the execution.
    NoValidation,
}

#[derive(Debug, Clone)]
pub enum ViolatedValidationRule {
    TouchedUnallowedStorageSlots(Address, U256),
    CalledContractWithNoCode(Address),
    TouchedUnallowedContext,
    TookTooManyComputationalGas(u32),
}

pub enum ValidationError {
    FailedTx(VmRevertReasonParsingResult),
    ViolatedRule(ViolatedValidationRule),
}

impl Display for ViolatedValidationRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViolatedValidationRule::TouchedUnallowedStorageSlots(contract, key) => write!(
                f,
                "Touched unallowed storage slots: address {}, key: {}",
                hex::encode(contract),
                hex::encode(u256_to_h256(*key))
            ),
            ViolatedValidationRule::CalledContractWithNoCode(contract) => {
                write!(f, "Called contract with no code: {}", hex::encode(contract))
            }
            ViolatedValidationRule::TouchedUnallowedContext => {
                write!(f, "Touched unallowed context")
            }
            ViolatedValidationRule::TookTooManyComputationalGas(gas_limit) => {
                write!(
                    f,
                    "Took too many computational gas, allowed limit: {}",
                    gas_limit
                )
            }
        }
    }
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FailedTx(revert_reason) => {
                write!(f, "Validation revert: {}", revert_reason.revert_reason)
            }
            Self::ViolatedRule(rule) => {
                write!(f, "Violated validation rules: {}", rule)
            }
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

/// Tracer that is used to ensure that the validation adheres to all the rules
/// to prevent DDoS attacks on the server.
#[derive(Debug, Clone)]
pub struct ValidationTracer<S: WriteStorage, H> {
    // A copy of it should be used in the Storage oracle
    pub storage: StoragePtr<S>,
    pub validation_mode: ValidationTracerMode,
    pub auxilary_allowed_slots: HashSet<H256>,
    pub validation_error: Option<ViolatedValidationRule>,

    user_address: Address,
    #[allow(dead_code)]
    paymaster_address: Address,
    should_stop_execution: bool,
    trusted_slots: HashSet<(Address, U256)>,
    trusted_addresses: HashSet<Address>,
    trusted_address_slots: HashSet<(Address, U256)>,
    computational_gas_used: u32,
    computational_gas_limit: u32,

    _marker: PhantomData<H>,
}

#[derive(Debug, Clone)]
pub struct ValidationTracerParams {
    pub user_address: Address,
    pub paymaster_address: Address,
    /// Slots that are trusted (i.e. the user can access them).
    pub trusted_slots: HashSet<(Address, U256)>,
    /// Trusted addresses (the user can access any slots on these addresses).
    pub trusted_addresses: HashSet<Address>,
    /// Slots, that are trusted and the value of them is the new trusted address.
    /// They are needed to work correctly with beacon proxy, where the address of the implementation is
    /// stored in the beacon.
    pub trusted_address_slots: HashSet<(Address, U256)>,
    /// Number of computational gas that validation step is allowed to use.
    pub computational_gas_limit: u32,
}

#[derive(Debug, Clone, Default)]
pub struct NewTrustedValidationItems {
    pub new_allowed_slots: Vec<H256>,
    pub new_trusted_addresses: Vec<Address>,
}

type ValidationRoundResult = Result<NewTrustedValidationItems, ViolatedValidationRule>;

impl<S: WriteStorage, H: HistoryMode> ValidationTracer<S, H> {
    pub fn new(storage: StoragePtr<S>, params: ValidationTracerParams) -> Self {
        ValidationTracer {
            storage,
            validation_error: None,
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

            _marker: PhantomData,
        }
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
                self.validation_error = Some(err);
            }
        }
    }

    // Checks whether such storage access is acceptable.
    fn is_allowed_storage_read(&self, address: Address, key: U256, msg_sender: Address) -> bool {
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

        if is_constant_code_hash(address, key, self.storage.clone()) {
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

    pub fn check_user_restrictions(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &SimpleMemory<H>,
    ) -> ValidationRoundResult {
        if self.computational_gas_used > self.computational_gas_limit {
            return Err(ViolatedValidationRule::TookTooManyComputationalGas(
                self.computational_gas_limit,
            ));
        }

        let opcode_variant = data.opcode.variant;
        match opcode_variant.opcode {
            Opcode::FarCall(_) => {
                let packed_abi = data.src0_value.value;
                let call_destination_value = data.src1_value.value;

                let called_address = u256_to_account_address(&call_destination_value);
                let far_call_abi = FarCallABI::from_u256(packed_abi);

                if called_address == KECCAK256_PRECOMPILE_ADDRESS
                    && far_call_abi.memory_quasi_fat_pointer.length == 64
                {
                    let calldata_page = get_calldata_page_via_abi(
                        &far_call_abi,
                        state.vm_local_state.callstack.current.base_memory_page,
                    );
                    let calldata = memory.read_unaligned_bytes(
                        calldata_page as usize,
                        far_call_abi.memory_quasi_fat_pointer.start as usize,
                        64,
                    );

                    let slot_to_add =
                        self.slot_to_add_from_keccak_call(&calldata, self.user_address);

                    if let Some(slot) = slot_to_add {
                        return Ok(NewTrustedValidationItems {
                            new_allowed_slots: vec![slot],
                            ..Default::default()
                        });
                    }
                } else if called_address != self.user_address {
                    let code_key = get_code_key(&called_address);
                    let code = self.storage.borrow_mut().read_value(&code_key);

                    if code == H256::zero() {
                        // The users are not allowed to call contracts with no code
                        return Err(ViolatedValidationRule::CalledContractWithNoCode(
                            called_address,
                        ));
                    }
                }
            }
            Opcode::Context(context) => {
                match context {
                    ContextOpcode::Meta => {
                        return Err(ViolatedValidationRule::TouchedUnallowedContext);
                    }
                    ContextOpcode::ErgsLeft => {
                        // TODO (SMA-1168): implement the correct restrictions for the gas left opcode.
                    }
                    _ => {}
                }
            }
            Opcode::Log(LogOpcode::StorageRead) => {
                let key = data.src0_value.value;
                let this_address = state.vm_local_state.callstack.current.this_address;
                let msg_sender = state.vm_local_state.callstack.current.msg_sender;

                if !self.is_allowed_storage_read(this_address, key, msg_sender) {
                    return Err(ViolatedValidationRule::TouchedUnallowedStorageSlots(
                        this_address,
                        key,
                    ));
                }

                if self.trusted_address_slots.contains(&(this_address, key)) {
                    let storage_key =
                        StorageKey::new(AccountTreeId::new(this_address), u256_to_h256(key));

                    let value = self.storage.borrow_mut().read_value(&storage_key);

                    return Ok(NewTrustedValidationItems {
                        new_trusted_addresses: vec![h256_to_account_address(&value)],
                        ..Default::default()
                    });
                }
            }
            _ => {}
        }

        Ok(Default::default())
    }
}

impl<S: WriteStorage, H: HistoryMode> Tracer for ValidationTracer<S, H> {
    const CALL_BEFORE_EXECUTION: bool = true;

    type SupportedMemory = SimpleMemory<H>;
    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: &Self::SupportedMemory) {}
    fn after_decoding(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterDecodingData,
        _memory: &Self::SupportedMemory,
    ) {
    }
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        // For now, we support only validations for users.
        if let ValidationTracerMode::UserTxValidation = self.validation_mode {
            self.computational_gas_used = self
                .computational_gas_used
                .saturating_add(computational_gas_price(state, &data));

            let validation_round_result = self.check_user_restrictions(state, data, memory);
            self.process_validation_round_result(validation_round_result);
        }

        let hook = VmHook::from_opcode_memory(&state, &data);
        print_debug_if_needed(&hook, &state, memory);

        let current_mode = self.validation_mode;
        match (current_mode, hook) {
            (ValidationTracerMode::NoValidation, VmHook::AccountValidationEntered) => {
                // Account validation can be entered when there is no prior validation (i.e. "nested" validations are not allowed)
                self.validation_mode = ValidationTracerMode::UserTxValidation;
            }
            (ValidationTracerMode::NoValidation, VmHook::PaymasterValidationEntered) => {
                // Paymaster validation can be entered when there is no prior validation (i.e. "nested" validations are not allowed)
                self.validation_mode = ValidationTracerMode::PaymasterTxValidation;
            }
            (_, VmHook::AccountValidationEntered | VmHook::PaymasterValidationEntered) => {
                panic!(
                    "Unallowed transition inside the validation tracer. Mode: {:#?}, hook: {:#?}",
                    self.validation_mode, hook
                );
            }
            (_, VmHook::NoValidationEntered) => {
                // Validation can be always turned off
                self.validation_mode = ValidationTracerMode::NoValidation;
            }
            (_, VmHook::ValidationStepEndeded) => {
                // The validation step has ended.
                self.should_stop_execution = true;
            }
            (_, _) => {
                // The hook is not relevant to the validation tracer. Ignore.
            }
        }
    }
    fn after_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        _memory: &Self::SupportedMemory,
    ) {
    }
}

impl<S: WriteStorage, H: HistoryMode> ExecutionEndTracer<H> for ValidationTracer<S, H> {
    fn should_stop_execution(&self) -> bool {
        self.should_stop_execution || self.validation_error.is_some()
    }
}

impl<S: WriteStorage, H: HistoryMode> PendingRefundTracer<H> for ValidationTracer<S, H> {}
impl<S: WriteStorage, H: HistoryMode> PubdataSpentTracer<H> for ValidationTracer<S, H> {}
impl<S: WriteStorage, H: HistoryMode> StorageInvocationTracer<H> for ValidationTracer<S, H> {}
