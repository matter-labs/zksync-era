use std::{collections::HashMap, fmt, sync::Arc};

use once_cell::sync::OnceCell;
use zksync_types::{Address, H256, U256};

pub mod vm_1_4_1;
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
