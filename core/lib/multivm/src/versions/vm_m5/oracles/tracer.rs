use std::fmt::Debug;
use std::{
    collections::HashSet,
    fmt::{self, Display},
};

use crate::vm_m5::{
    errors::VmRevertReasonParsingResult,
    memory::SimpleMemory,
    storage::StoragePtr,
    utils::{aux_heap_page_from_base, heap_page_from_base},
    vm_instance::{get_vm_hook_params, VM_HOOK_POSITION},
    vm_with_bootloader::BOOTLOADER_HEAP_PAGE,
};
// use zk_evm_1_3_1::testing::memory::SimpleMemory;
use crate::vm_m5::storage::Storage;
use zk_evm_1_3_1::{
    abstractions::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    aux_structures::MemoryPage,
    vm_state::{ErrorFlags, VmLocalState},
    witness_trace::{DummyTracer, VmWitnessTracer},
    zkevm_opcode_defs::{
        decoding::VmEncodingMode, ContextOpcode, FarCallABI, FarCallForwardPageType, FatPointer,
        LogOpcode, Opcode, RetOpcode, UMAOpcode,
    },
};

use zksync_types::{
    get_code_key, web3::signing::keccak256, AccountTreeId, Address, StorageKey,
    ACCOUNT_CODE_STORAGE_ADDRESS, BOOTLOADER_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, H256,
    KECCAK256_PRECOMPILE_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L1_MESSENGER_ADDRESS,
    L2_ETH_TOKEN_ADDRESS, MSG_VALUE_SIMULATOR_ADDRESS, SYSTEM_CONTEXT_ADDRESS, U256,
};
use zksync_utils::{
    be_bytes_to_safe_address, h256_to_account_address, u256_to_account_address, u256_to_h256,
};

pub trait ExecutionEndTracer: Tracer<SupportedMemory = SimpleMemory> {
    // Returns whether the vm execution should stop.
    fn should_stop_execution(&self) -> bool;
}

pub trait PendingRefundTracer: Tracer<SupportedMemory = SimpleMemory> {
    // Some(x) means that the bootloader has asked the operator to provide the refund for the
    // transaction, where `x` is the refund that the bootloader has suggested on its own.
    fn requested_refund(&self) -> Option<u32> {
        None
    }

    // Set the current request for refund as fulfilled
    fn set_refund_as_done(&mut self) {}
}
pub trait PubdataSpentTracer: Tracer<SupportedMemory = SimpleMemory> {
    // Returns how much gas was spent on pubdata.
    fn gas_spent_on_pubdata(&self, _vm_local_state: &VmLocalState) -> u32 {
        0
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TransactionResultTracer {
    pub(crate) revert_reason: Option<Vec<u8>>,
}

impl<const N: usize, E: VmEncodingMode<N>> VmWitnessTracer<N, E> for TransactionResultTracer {}

impl Tracer for TransactionResultTracer {
    type SupportedMemory = SimpleMemory;
    const CALL_BEFORE_EXECUTION: bool = true;

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
        let hook = VmHook::from_opcode_memory(&state, &data);
        print_debug_if_needed(&hook, &state, memory);

        if matches!(hook, VmHook::ExecutionResult) {
            let vm_hook_params = get_vm_hook_params(memory);

            let success = vm_hook_params[0];
            let returndata_ptr = FatPointer::from_u256(vm_hook_params[1]);
            let returndata = read_pointer(memory, returndata_ptr);

            if success == U256::zero() {
                self.revert_reason = Some(returndata);
            } else {
                self.revert_reason = None;
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

impl ExecutionEndTracer for TransactionResultTracer {
    fn should_stop_execution(&self) -> bool {
        // This tracer will not prevent the execution from going forward
        // until the end of the block.
        false
    }
}

impl PendingRefundTracer for TransactionResultTracer {}
impl PubdataSpentTracer for TransactionResultTracer {}

#[derive(Debug, Clone, Eq, PartialEq, Copy)]
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
}

pub enum ValidationError {
    FailedTx(VmRevertReasonParsingResult),
    VioalatedRule(ViolatedValidationRule),
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
        }
    }
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FailedTx(revert_reason) => {
                write!(f, "Validation revert: {}", revert_reason.revert_reason)
            }
            Self::VioalatedRule(rule) => {
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

fn is_constant_code_hash<S: Storage>(address: Address, key: U256, storage: StoragePtr<S>) -> bool {
    if address != ACCOUNT_CODE_STORAGE_ADDRESS {
        // Not a code hash
        return false;
    }

    let value = storage.borrow_mut().get_value(&StorageKey::new(
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
#[derive(Clone)]
pub struct ValidationTracer<S> {
    // A copy of it should be used in the Storage oracle
    pub storage: StoragePtr<S>,
    pub validation_mode: ValidationTracerMode,
    pub auxilary_allowed_slots: HashSet<H256>,
    pub validation_error: Option<ViolatedValidationRule>,

    user_address: Address,
    paymaster_address: Address,
    should_stop_execution: bool,
    trusted_slots: HashSet<(Address, U256)>,
    trusted_addresses: HashSet<Address>,
    trusted_address_slots: HashSet<(Address, U256)>,
}

impl<S> fmt::Debug for ValidationTracer<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidationTracer")
            .field("storage", &"StoragePtr")
            .field("validation_mode", &self.validation_mode)
            .field("auxilary_allowed_slots", &self.auxilary_allowed_slots)
            .field("validation_error", &self.validation_error)
            .field("user_address", &self.user_address)
            .field("paymaster_address", &self.paymaster_address)
            .field("should_stop_execution", &self.should_stop_execution)
            .field("trusted_slots", &self.trusted_slots)
            .field("trusted_addresses", &self.trusted_addresses)
            .field("trusted_address_slots", &self.trusted_address_slots)
            .finish()
    }
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
}

#[derive(Debug, Clone, Default)]
pub struct NewTrustedValidationItems {
    pub new_allowed_slots: Vec<H256>,
    pub new_trusted_addresses: Vec<Address>,
}

type ValidationRoundResult = Result<NewTrustedValidationItems, ViolatedValidationRule>;

impl<S: Storage> ValidationTracer<S> {
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
        memory: &SimpleMemory,
    ) -> ValidationRoundResult {
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
                    let code = self.storage.borrow_mut().get_value(&code_key);

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

                    let value = self.storage.borrow_mut().get_value(&storage_key);

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

impl<S: Storage> Tracer for ValidationTracer<S> {
    const CALL_BEFORE_EXECUTION: bool = true;

    type SupportedMemory = SimpleMemory;
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

fn get_calldata_page_via_abi(far_call_abi: &FarCallABI, base_page: MemoryPage) -> u32 {
    match far_call_abi.forwarding_mode {
        FarCallForwardPageType::ForwardFatPointer => {
            far_call_abi.memory_quasi_fat_pointer.memory_page
        }
        FarCallForwardPageType::UseAuxHeap => aux_heap_page_from_base(base_page).0,
        FarCallForwardPageType::UseHeap => heap_page_from_base(base_page).0,
    }
}

impl<S: Storage> ExecutionEndTracer for ValidationTracer<S> {
    fn should_stop_execution(&self) -> bool {
        self.should_stop_execution || self.validation_error.is_some()
    }
}

impl<S: Storage> PendingRefundTracer for ValidationTracer<S> {}
impl<S: Storage> PubdataSpentTracer for ValidationTracer<S> {}

/// Allows any opcodes, but tells the VM to end the execution once the tx is over.
#[derive(Debug, Clone, Default)]
pub struct OneTxTracer {
    tx_has_been_processed: bool,

    // Some(x) means that the bootloader has asked the operator
    // to provide the refund the user, where `x` is the refund proposed
    // by the bootloader itself.
    pending_operator_refund: Option<u32>,

    pub refund_gas: u32,
    pub gas_spent_on_bytecodes_and_long_messages: u32,
    bootloader_tracer: BootloaderTracer,
}

impl Tracer for OneTxTracer {
    const CALL_BEFORE_EXECUTION: bool = true;
    const CALL_AFTER_EXECUTION: bool = true;
    type SupportedMemory = SimpleMemory;

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
        let hook = VmHook::from_opcode_memory(&state, &data);
        print_debug_if_needed(&hook, &state, memory);

        match hook {
            VmHook::TxHasEnded => self.tx_has_been_processed = true,
            VmHook::NotifyAboutRefund => self.refund_gas = get_vm_hook_params(memory)[0].as_u32(),
            VmHook::AskOperatorForRefund => {
                self.pending_operator_refund = Some(get_vm_hook_params(memory)[0].as_u32())
            }
            _ => {}
        }

        if data.opcode.variant.opcode == Opcode::Log(LogOpcode::PrecompileCall) {
            let current_stack = state.vm_local_state.callstack.get_current_stack();
            // Trace for precompile calls from `KNOWN_CODES_STORAGE_ADDRESS` and `L1_MESSENGER_ADDRESS` that burn some gas.
            // Note, that if there is less gas left than requested to burn it will be burnt anyway.
            if current_stack.this_address == KNOWN_CODES_STORAGE_ADDRESS
                || current_stack.this_address == L1_MESSENGER_ADDRESS
            {
                self.gas_spent_on_bytecodes_and_long_messages +=
                    std::cmp::min(data.src1_value.value.as_u32(), current_stack.ergs_remaining);
            }
        }
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        self.bootloader_tracer.after_execution(state, data, memory)
    }
}

impl ExecutionEndTracer for OneTxTracer {
    fn should_stop_execution(&self) -> bool {
        self.tx_has_been_processed || self.bootloader_tracer.should_stop_execution()
    }
}

impl PendingRefundTracer for OneTxTracer {
    fn requested_refund(&self) -> Option<u32> {
        self.pending_operator_refund
    }

    fn set_refund_as_done(&mut self) {
        self.pending_operator_refund = None;
    }
}

impl PubdataSpentTracer for OneTxTracer {
    fn gas_spent_on_pubdata(&self, vm_local_state: &VmLocalState) -> u32 {
        self.gas_spent_on_bytecodes_and_long_messages + vm_local_state.spent_pubdata_counter
    }
}

impl OneTxTracer {
    pub fn is_bootloader_out_of_gas(&self) -> bool {
        self.bootloader_tracer.is_bootloader_out_of_gas()
    }

    pub fn tx_has_been_processed(&self) -> bool {
        self.tx_has_been_processed
    }
}

/// Tells the VM to end the execution before `ret` from the booloader if there is no panic or revert.
/// Also, saves the information if this `ret` was caused by "out of gas" panic.
#[derive(Debug, Clone, Default)]
pub struct BootloaderTracer {
    is_bootloader_out_of_gas: bool,
    ret_from_the_bootloader: Option<RetOpcode>,
}

impl Tracer for BootloaderTracer {
    const CALL_AFTER_DECODING: bool = true;
    const CALL_AFTER_EXECUTION: bool = true;
    type SupportedMemory = SimpleMemory;

    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: &Self::SupportedMemory) {}
    fn after_decoding(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterDecodingData,
        _memory: &Self::SupportedMemory,
    ) {
        // We should check not only for the `NOT_ENOUGH_ERGS` flag but if the current frame is bootloader too.
        if Self::current_frame_is_bootloader(state.vm_local_state)
            && data
                .error_flags_accumulated
                .contains(ErrorFlags::NOT_ENOUGH_ERGS)
        {
            self.is_bootloader_out_of_gas = true;
        }
    }

    fn before_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: BeforeExecutionData,
        _memory: &Self::SupportedMemory,
    ) {
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        // Decodes next opcode.
        // `self` is passed as `tracer`, so `self.after_decoding` will be called and it will catch "out of gas".
        let (next_opcode, _, _) = zk_evm_1_3_1::vm_state::read_and_decode(
            state.vm_local_state,
            memory,
            &mut DummyTracer,
            self,
        );
        if Self::current_frame_is_bootloader(state.vm_local_state) {
            if let Opcode::Ret(ret) = next_opcode.inner.variant.opcode {
                self.ret_from_the_bootloader = Some(ret);
            }
        }
    }
}

impl ExecutionEndTracer for BootloaderTracer {
    fn should_stop_execution(&self) -> bool {
        self.ret_from_the_bootloader == Some(RetOpcode::Ok)
    }
}

impl PendingRefundTracer for BootloaderTracer {}
impl PubdataSpentTracer for BootloaderTracer {}

impl BootloaderTracer {
    fn current_frame_is_bootloader(local_state: &VmLocalState) -> bool {
        // The current frame is bootloader if the callstack depth is 1.
        // Some of the near calls inside the bootloader can be out of gas, which is totally normal behavior
        // and it shouldn't result in `is_bootloader_out_of_gas` becoming true.
        local_state.callstack.inner.len() == 1
    }

    pub fn is_bootloader_out_of_gas(&self) -> bool {
        self.is_bootloader_out_of_gas
    }

    pub fn bootloader_panicked(&self) -> bool {
        self.ret_from_the_bootloader == Some(RetOpcode::Panic)
    }
}

#[derive(Clone, Debug, Copy)]
pub(crate) enum VmHook {
    AccountValidationEntered,
    PaymasterValidationEntered,
    NoValidationEntered,
    ValidationStepEndeded,
    TxHasEnded,
    DebugLog,
    DebugReturnData,
    NoHook,
    NearCallCatch,
    AskOperatorForRefund,
    NotifyAboutRefund,
    ExecutionResult,
}

impl VmHook {
    pub fn from_opcode_memory(state: &VmLocalStateData<'_>, data: &BeforeExecutionData) -> Self {
        let opcode_variant = data.opcode.variant;
        let heap_page =
            heap_page_from_base(state.vm_local_state.callstack.current.base_memory_page).0;

        let src0_value = data.src0_value.value;

        let fat_ptr = FatPointer::from_u256(src0_value);

        let value = data.src1_value.value;

        // Only UMA opcodes in the bootloader serve for vm hooks
        if !matches!(opcode_variant.opcode, Opcode::UMA(UMAOpcode::HeapWrite))
            || heap_page != BOOTLOADER_HEAP_PAGE
            || fat_ptr.offset != VM_HOOK_POSITION * 32
        {
            return Self::NoHook;
        }

        match value.as_u32() {
            0 => Self::AccountValidationEntered,
            1 => Self::PaymasterValidationEntered,
            2 => Self::NoValidationEntered,
            3 => Self::ValidationStepEndeded,
            4 => Self::TxHasEnded,
            5 => Self::DebugLog,
            6 => Self::DebugReturnData,
            7 => Self::NearCallCatch,
            8 => Self::AskOperatorForRefund,
            9 => Self::NotifyAboutRefund,
            10 => Self::ExecutionResult,
            _ => panic!("Unknown hook: {}", value.as_u32()),
        }
    }
}

fn get_debug_log(state: &VmLocalStateData<'_>, memory: &SimpleMemory) -> String {
    let vm_hook_params: Vec<_> = get_vm_hook_params(memory)
        .into_iter()
        .map(u256_to_h256)
        .collect();
    let msg = vm_hook_params[0].as_bytes().to_vec();
    let data = vm_hook_params[1].as_bytes().to_vec();

    let msg = String::from_utf8(msg).expect("Invalid debug message");
    let data = U256::from_big_endian(&data);

    // For long data, it is better to use hex-encoding for greater readibility
    let data_str = if data > U256::from(u64::max_value()) {
        let mut bytes = [0u8; 32];
        data.to_big_endian(&mut bytes);
        format!("0x{}", hex::encode(bytes))
    } else {
        data.to_string()
    };

    let tx_id = state.vm_local_state.tx_number_in_block;

    format!("Bootloader transaction {}: {} {}", tx_id, msg, data_str)
}

/// Reads the memory slice represented by the fat pointer.
/// Note, that the fat pointer must point to the accesible memory (i.e. not cleared up yet).
pub(crate) fn read_pointer(memory: &SimpleMemory, pointer: FatPointer) -> Vec<u8> {
    let FatPointer {
        offset,
        length,
        start,
        memory_page,
    } = pointer;

    // The actual bounds of the returndata ptr is [start+offset..start+length]
    let mem_region_start = start + offset;
    let mem_region_length = length - offset;

    memory.read_unaligned_bytes(
        memory_page as usize,
        mem_region_start as usize,
        mem_region_length as usize,
    )
}

/// Outputs the returndata for the latest call.
/// This is usually used to output the revert reason.
fn get_debug_returndata(memory: &SimpleMemory) -> String {
    let vm_hook_params: Vec<_> = get_vm_hook_params(memory);
    let returndata_ptr = FatPointer::from_u256(vm_hook_params[0]);
    let returndata = read_pointer(memory, returndata_ptr);

    format!("0x{}", hex::encode(returndata))
}

/// Accepts a vm hook and, if it requires to output some debug log, outputs it.
fn print_debug_if_needed(hook: &VmHook, state: &VmLocalStateData<'_>, memory: &SimpleMemory) {
    let log = match hook {
        VmHook::DebugLog => get_debug_log(state, memory),
        VmHook::DebugReturnData => get_debug_returndata(memory),
        _ => return,
    };

    tracing::trace!("{}", log);
}
