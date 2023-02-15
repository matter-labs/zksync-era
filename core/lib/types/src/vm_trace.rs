use crate::{Address, U256};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
