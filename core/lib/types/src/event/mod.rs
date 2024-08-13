use once_cell::sync::Lazy;

use crate::{ethabi, H256};

// FIXME: ???
pub static DEPLOY_EVENT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "ContractDeployed",
        &[
            ethabi::ParamType::Address,
            ethabi::ParamType::FixedBytes(32),
            ethabi::ParamType::Address,
        ],
    )
});

// FIXME: ???
pub static L1_MESSENGER_BYTECODE_PUBLICATION_EVENT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "BytecodeL1PublicationRequested",
        &[ethabi::ParamType::FixedBytes(32)],
    )
});
