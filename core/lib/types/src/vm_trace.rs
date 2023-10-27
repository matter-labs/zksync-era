use crate::{Address, U256};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::Display;
use zk_evm::zkevm_opcode_defs::FarCallOpcode;
use zksync_system_constants::BOOTLOADER_ADDRESS;
use zksync_utils::u256_to_h256;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum VmTrace {
    ExecutionTrace(VmExecutionTrace),
    CallTrace(Vec<Call>),
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct VmExecutionTrace {
    pub steps: Vec<VmExecutionStep>,
    pub contracts: HashSet<Address>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct VmExecutionStep {
    pub contract_address: Address,
    pub memory_page_index: usize,
    pub child_memory_index: usize,
    pub pc: u16,
    pub set_flags: Vec<String>,
    pub registers: Vec<U256>,
    pub register_interactions: HashMap<u8, MemoryDirection>,
    pub sp: Option<u16>,
    pub memory_interactions: Vec<MemoryInteraction>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MemoryInteraction {
    pub memory_type: String,
    pub page: usize,
    pub address: u16,
    pub value: U256,
    pub direction: MemoryDirection,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum MemoryDirection {
    Read,
    Write,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContractSourceDebugInfo {
    pub assembly_code: String,
    pub pc_line_mapping: HashMap<usize, usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VmDebugTrace {
    pub steps: Vec<VmExecutionStep>,
    pub sources: HashMap<Address, Option<ContractSourceDebugInfo>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum CallType {
    #[serde(serialize_with = "far_call_type_to_u8")]
    #[serde(deserialize_with = "far_call_type_from_u8")]
    Call(FarCallOpcode),
    Create,
    NearCall,
}

#[derive(Clone, Serialize, Deserialize)]
/// Represents a call in the VM trace.
pub struct Call {
    /// Type of the call.
    pub r#type: CallType,
    /// Address of the caller.
    pub from: Address,
    /// Address of the callee.
    pub to: Address,
    /// Gas from the parent call.
    pub parent_gas: u32,
    /// Gas provided for the call.
    pub gas: u32,
    /// Gas used by the call.
    pub gas_used: u32,
    /// Value transferred.
    pub value: U256,
    /// Input data.
    pub input: Vec<u8>,
    /// Output data.
    pub output: Vec<u8>,
    /// Error message provided by vm or some unexpected errors.
    pub error: Option<String>,
    /// Revert reason.
    pub revert_reason: Option<String>,
    /// Subcalls.
    pub calls: Vec<Call>,
}

impl Call {
    pub fn new_high_level(
        gas: u32,
        gas_used: u32,
        value: U256,
        input: Vec<u8>,
        output: Vec<u8>,
        revert_reason: Option<String>,
        calls: Vec<Call>,
    ) -> Self {
        Self {
            r#type: CallType::Call(FarCallOpcode::Normal),
            from: Address::zero(),
            to: BOOTLOADER_ADDRESS,
            parent_gas: gas,
            gas,
            gas_used,
            value,
            input,
            output,
            error: None,
            revert_reason,
            calls,
        }
    }
}

impl PartialEq for Call {
    fn eq(&self, other: &Self) -> bool {
        self.revert_reason == other.revert_reason
            && self.input == other.input
            && self.from == other.from
            && self.to == other.to
            && self.r#type == other.r#type
            && self.value == other.value
            && self.error == other.error
            && self.output == other.output
            && self.calls == other.calls
    }
}

fn far_call_type_from_u8<'de, D>(deserializer: D) -> Result<FarCallOpcode, D::Error>
where
    D: Deserializer<'de>,
{
    let res = u8::deserialize(deserializer)?;
    match res {
        0 => Ok(FarCallOpcode::Normal),
        1 => Ok(FarCallOpcode::Delegate),
        2 => Ok(FarCallOpcode::Mimic),
        _ => Err(serde::de::Error::custom("Invalid FarCallOpcode")),
    }
}

fn far_call_type_to_u8<S>(far_call_type: &FarCallOpcode, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_u8(*far_call_type as u8)
}

impl Default for Call {
    fn default() -> Self {
        Self {
            r#type: CallType::Call(FarCallOpcode::Normal),
            from: Default::default(),
            to: Default::default(),
            parent_gas: 0,
            gas: 0,
            gas_used: 0,
            value: Default::default(),
            input: vec![],
            output: vec![],
            error: None,
            revert_reason: None,
            calls: vec![],
        }
    }
}

impl fmt::Debug for Call {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Call")
            .field("type", &self.r#type)
            .field("to", &self.to)
            .field("from", &self.from)
            .field("parent_gas", &self.parent_gas)
            .field("gas_used", &self.gas_used)
            .field("gas", &self.gas)
            .field("value", &self.value)
            .field("input", &format_args!("{:?}", self.input))
            .field("output", &format_args!("{:?}", self.output))
            .field("error", &self.error)
            .field("revert_reason", &format_args!("{:?}", self.revert_reason))
            .field("call_traces", &self.calls)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum ViolatedValidationRule {
    TouchedUnallowedStorageSlots(Address, U256),
    CalledContractWithNoCode(Address),
    TouchedUnallowedContext,
    TookTooManyComputationalGas(u32),
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
