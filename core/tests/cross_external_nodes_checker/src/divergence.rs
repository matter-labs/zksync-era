use std::fmt;
use zksync_types::{web3::types::U64, MiniblockNumber};

#[derive(Debug, Clone)]
pub(crate) enum Divergence {
    BatchDetails(DivergenceDetails<Option<String>>),
    MiniblockDetails(DivergenceDetails<Option<String>>),
    Transaction(DivergenceDetails<Option<String>>),
    TransactionReceipt(DivergenceDetails<Option<String>>),
    TransactionDetails(DivergenceDetails<Option<String>>),
    Log(DivergenceDetails<Option<String>>),
    MainContracts(DivergenceDetails<Option<String>>),
    BridgeContracts(DivergenceDetails<Option<String>>),
    ChainID(DivergenceDetails<Option<U64>>),
    L1ChainID(DivergenceDetails<Option<U64>>),
    PubSubHeader(DivergenceDetails<Option<String>>),
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Divergence::BatchDetails(details) => {
                write!(f, "Batch Details divergence found: {}", details)
            }
            Divergence::MiniblockDetails(details) => {
                write!(f, "Miniblock Details divergence found: {}", details)
            }
            Divergence::Transaction(details) => {
                write!(f, "Transaction divergence found: {}", details)
            }
            Divergence::TransactionReceipt(details) => {
                write!(f, "TransactionReceipt divergence found: {}", details)
            }
            Divergence::TransactionDetails(details) => {
                write!(f, "TransactionDetails divergence found: {}", details)
            }
            Divergence::Log(details) => write!(f, "Log divergence found: {}", details),
            Divergence::MainContracts(details) => {
                write!(f, "MainContracts divergence found: {}", details)
            }
            Divergence::BridgeContracts(details) => {
                write!(f, "BridgeContracts divergence found: {}", details)
            }
            Divergence::ChainID(details) => write!(f, "ChainID divergence found: {}", details),
            Divergence::L1ChainID(details) => {
                write!(f, "L1ChainID divergence found: {}", details)
            }
            Divergence::PubSubHeader(details) => {
                write!(f, "PubSubHeader divergence found: {}", details)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DivergenceDetails<T> {
    pub(crate) en_instance_url: String,
    pub(crate) main_node_value: T,
    pub(crate) en_instance_value: T,
    pub(crate) entity_id: Option<String>,
    pub(crate) miniblock_number: Option<MiniblockNumber>,
}

impl<T: fmt::Display> fmt::Display for DivergenceDetails<Option<T>> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let main_node_value = match &self.main_node_value {
            Some(value) => format!("{}", value),
            None => String::from("None"),
        };
        let en_instance_value = match &self.en_instance_value {
            Some(value) => format!("{}", value),
            None => String::from("None"),
        };
        let entity_info = match self.entity_id {
            Some(ref entity_id) => format!(", Entity ID: {}", entity_id),
            None => String::from(""),
        };
        let miniblock_number = match self.miniblock_number {
            Some(ref number) => format!(", Miniblock number: {}", number),
            None => String::from(""),
        };
        write!(
            f,
            "Main node value: {}, EN instance value: {}{} in EN instance: {}{}",
            main_node_value, en_instance_value, miniblock_number, self.en_instance_url, entity_info
        )
    }
}
