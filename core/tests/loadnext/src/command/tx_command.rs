use once_cell::sync::OnceCell;
use rand::Rng;
use static_assertions::const_assert;
use zksync_types::{Address, U256};

use crate::{
    account_pool::AddressPool,
    all::{All, AllWeighted},
    config::TransactionWeights,
    rng::{LoadtestRng, WeightedRandom},
};

static WEIGHTS: OnceCell<[(TxType, f32); 5]> = OnceCell::new();

/// Type of transaction. It doesn't copy the ZKsync operation list, because
/// it divides some transactions in subcategories (e.g. to new account / to existing account; to self / to other; etc)/
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TxType {
    Deposit,
    WithdrawToSelf,
    WithdrawToOther,
    DeployContract,
    L1Execute,
    L2Execute,
}

impl TxType {
    pub fn initialize_weights(transaction_weights: &TransactionWeights) {
        WEIGHTS
            .set([
                (TxType::Deposit, transaction_weights.deposit),
                (TxType::L2Execute, transaction_weights.l2_transactions),
                (TxType::L1Execute, transaction_weights.l1_transactions),
                (TxType::WithdrawToSelf, transaction_weights.withdrawal / 2.0),
                (
                    TxType::WithdrawToOther,
                    transaction_weights.withdrawal / 2.0,
                ),
            ])
            .unwrap();
    }
}

impl All for TxType {
    fn all() -> &'static [Self] {
        Self::const_all()
    }
}

impl AllWeighted for TxType {
    fn all_weighted() -> &'static [(Self, f32)] {
        WEIGHTS.get().expect("Weights are not initialized")
    }
}

impl TxType {
    const fn const_all() -> &'static [Self] {
        &[
            Self::Deposit,
            Self::WithdrawToSelf,
            Self::WithdrawToOther,
            Self::L1Execute,
            Self::L2Execute,
        ]
    }

    fn is_target_self(self) -> bool {
        matches!(self, Self::WithdrawToSelf)
    }
}

/// Modifier to be applied to the transaction in order to make it incorrect.
///
/// Incorrect transactions are a significant part of loadtest, because we want to ensure
/// that server is resilient for all the possible kinds of user input.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum IncorrectnessModifier {
    ZeroFee,
    IncorrectSignature,

    // Last option goes for no modifier,
    // since it's more convenient than dealing with `Option<IncorrectnessModifier>`.
    None,
}

impl IncorrectnessModifier {
    // Have to implement this as a const function, since const functions in traits are not stabilized yet.
    const fn const_all() -> &'static [Self] {
        &[Self::ZeroFee, Self::IncorrectSignature, Self::None]
    }
}

impl All for IncorrectnessModifier {
    fn all() -> &'static [Self] {
        Self::const_all()
    }
}

impl AllWeighted for IncorrectnessModifier {
    fn all_weighted() -> &'static [(Self, f32)] {
        const VARIANT_AMOUNTS: f32 = IncorrectnessModifier::const_all().len() as f32;
        // No modifier is 9 times probable than all the other variants in sum.
        // In other words, 90% probability of no modifier.
        const NONE_PROBABILITY: f32 = (VARIANT_AMOUNTS - 1.0) * 9.0;
        const DEFAULT_PROBABILITY: f32 = 1.0f32;

        const WEIGHTED: &[(IncorrectnessModifier, f32)] = &[
            (IncorrectnessModifier::ZeroFee, DEFAULT_PROBABILITY),
            (
                IncorrectnessModifier::IncorrectSignature,
                DEFAULT_PROBABILITY,
            ),
            (IncorrectnessModifier::None, NONE_PROBABILITY),
        ];

        const_assert!(WEIGHTED.len() == IncorrectnessModifier::const_all().len());
        WEIGHTED
    }
}

/// Expected outcome of transaction:
/// Since we may create erroneous transactions on purpose,
/// we may expect different outcomes for each transaction.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ExpectedOutcome {
    /// Transactions was successfully executed.
    TxSucceed,
    /// Transaction sending should fail.
    ApiRequestFailed,
    /// Transaction should be accepted, but rejected at the
    /// time of execution.
    TxRejected,
}

impl IncorrectnessModifier {
    pub fn expected_outcome(self) -> ExpectedOutcome {
        match self {
            Self::None => ExpectedOutcome::TxSucceed,
            Self::ZeroFee | Self::IncorrectSignature => ExpectedOutcome::ApiRequestFailed,
        }
    }
}

/// Complete description of a transaction that must be executed by a test wallet.
#[derive(Debug, Clone)]
pub struct TxCommand {
    /// Type of operation.
    pub command_type: TxType,
    /// Whether and how transaction should be corrupted.
    pub modifier: IncorrectnessModifier,
    /// Recipient address.
    pub to: Address,
    /// Transaction amount (0 if not applicable).
    pub amount: U256,
}

impl TxCommand {
    /// Generates a fully random transaction command.
    pub fn random(rng: &mut LoadtestRng, own_address: Address, addresses: &AddressPool) -> Self {
        let command_type = TxType::random(rng);

        Self::new_with_type(rng, own_address, addresses, command_type)
    }

    fn new_with_type(
        rng: &mut LoadtestRng,
        own_address: Address,
        addresses: &AddressPool,
        command_type: TxType,
    ) -> Self {
        let mut command = Self {
            command_type,
            modifier: IncorrectnessModifier::random(rng),
            to: addresses.random_address(rng),
            amount: Self::random_amount(rng),
        };

        // Check whether we should use a self as a target.
        if command.command_type.is_target_self() {
            command.to = own_address;
        }

        // Fix incorrectness modifier:
        // L1 txs should always have `None` modifier.
        if matches!(command.command_type, TxType::Deposit | TxType::L1Execute) {
            command.modifier = IncorrectnessModifier::None;
        }

        command
    }

    fn random_amount(rng: &mut LoadtestRng) -> U256 {
        rng.gen_range(1u64..2u64.pow(18)).into()
    }
}
