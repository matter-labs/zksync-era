//! Types shared by multiple (usually old) VMs.

use std::collections::{HashMap, HashSet};

use zksync_types::{Address, U256};

use crate::interface::Call;

#[derive(Debug, Clone, PartialEq)]
pub enum VmTrace {
    ExecutionTrace(VmExecutionTrace),
    CallTrace(Vec<Call>),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VmExecutionTrace {
    pub steps: Vec<VmExecutionStep>,
    pub contracts: HashSet<Address>,
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryInteraction {
    pub memory_type: String,
    pub page: usize,
    pub address: u16,
    pub value: U256,
    pub direction: MemoryDirection,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemoryDirection {
    Read,
    Write,
}
