use zksync_types::{Address, H256};

#[derive(Debug, Clone, Eq, PartialEq, Copy)]
#[allow(clippy::enum_variant_names)]
pub(super) enum ValidationTracerMode {
    /// Should be activated when the transaction is being validated by user.
    UserTxValidation,
    /// Should be activated when the transaction is being validated by the paymaster.
    PaymasterTxValidation,
    /// Is a state when there are no restrictions on the execution.
    NoValidation,
}

#[derive(Debug, Clone, Default)]
pub(super) struct NewTrustedValidationItems {
    pub(super) new_allowed_slots: Vec<H256>,
    pub(super) new_trusted_addresses: Vec<Address>,
}
