use once_cell::sync::Lazy;

use crate::{ethabi, H256};

pub static L1_MESSENGER_BYTECODE_PUBLICATION_EVENT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "BytecodeL1PublicationRequested",
        &[ethabi::ParamType::FixedBytes(32)],
    )
});

pub static MESSAGE_ROOT_ADDED_CHAIN_EVENT: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "AddedChain",
        &[ethabi::ParamType::Uint(256), ethabi::ParamType::Uint(256)],
    )
});

pub static MESSAGE_ROOT_ADDED_CHAIN_BATCH_ROOT_EVENT: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "AppendedChainBatchRoot",
        &[
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::FixedBytes(32),
        ],
    )
});
