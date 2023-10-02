use std::time::Duration;

use zksync_types::Address;

use crate::account::ExecutionType;
use crate::{
    all::All,
    command::{ApiRequest, ApiRequestType, SubscriptionType, TxCommand, TxType},
};

/// Report for any operation done by loadtest.
///
/// Reports are yielded by `Executor` or `AccountLifespan` and are collected
/// by the `ReportCollector`.
///
/// Reports are expected to contain any kind of information useful for the analysis
/// and deciding whether the test was passed.
#[derive(Debug, Clone)]
pub struct Report {
    /// Address of the wallet that performed the action.
    pub reporter: Address,
    /// Obtained outcome of action.
    pub label: ReportLabel,
    /// Type of the action.
    pub action: ActionType,
    /// Amount of retries that it took the wallet to finish the action.
    pub retries: usize,
    /// Duration of the latest execution attempt.
    pub time: Duration,
}

/// Builder structure for `Report`.
#[derive(Debug, Clone)]
pub struct ReportBuilder {
    report: Report,
}

impl Default for ReportBuilder {
    fn default() -> Self {
        Self {
            report: Report {
                reporter: Address::default(),
                label: ReportLabel::done(),
                action: ActionType::Tx(TxActionType::Execute(ExecutionType::L2)),
                retries: 0,
                time: Duration::ZERO,
            },
        }
    }
}

impl ReportBuilder {
    pub fn reporter(mut self, reporter: Address) -> Self {
        self.report.reporter = reporter;
        self
    }

    pub fn label(mut self, label: ReportLabel) -> Self {
        self.report.label = label;
        self
    }

    pub fn action(mut self, action: impl Into<ActionType>) -> Self {
        self.report.action = action.into();
        self
    }

    pub fn retries(mut self, retries: usize) -> Self {
        self.report.retries = retries;
        self
    }

    pub fn time(mut self, time: Duration) -> Self {
        self.report.time = time;
        self
    }

    pub fn finish(self) -> Report {
        self.report
    }

    pub fn build_init_complete_report() -> Report {
        Report {
            reporter: Address::default(),
            label: ReportLabel::done(),
            action: ActionType::InitComplete,
            retries: 0,
            time: Duration::ZERO,
        }
    }
}

/// Denotes the outcome of a performed action.
#[derive(Debug, Clone)]
pub enum ReportLabel {
    ActionDone,
    ActionSkipped { reason: String },
    ActionFailed { error: String },
}

impl ReportLabel {
    pub fn done() -> Self {
        Self::ActionDone
    }

    pub fn skipped(reason: impl Into<String>) -> Self {
        Self::ActionSkipped {
            reason: reason.into(),
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self::ActionFailed {
            error: error.into(),
        }
    }
}

/// Denotes the type of executed transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TxActionType {
    Withdraw,
    Deposit,
    DeployContract,
    Execute(ExecutionType),
}

impl All for TxActionType {
    fn all() -> &'static [Self] {
        const ALL: &[TxActionType] = &[
            TxActionType::Withdraw,
            TxActionType::Deposit,
            TxActionType::DeployContract,
            TxActionType::Execute(ExecutionType::L2),
            TxActionType::Execute(ExecutionType::L1),
        ];

        ALL
    }
}

impl From<TxType> for TxActionType {
    fn from(command: TxType) -> Self {
        match command {
            TxType::Deposit => Self::Deposit,
            TxType::WithdrawToSelf | TxType::WithdrawToOther => Self::Withdraw,
            TxType::L2Execute => Self::Execute(ExecutionType::L2),
            TxType::L1Execute => Self::Execute(ExecutionType::L1),
            TxType::DeployContract => Self::DeployContract,
        }
    }
}

/// Denotes the type of the performed API action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiActionType {
    BlockWithTxs,
    Balance,
    GetLogs,
}

impl All for ApiActionType {
    fn all() -> &'static [Self] {
        const ALL: &[ApiActionType] = &[
            ApiActionType::BlockWithTxs,
            ApiActionType::Balance,
            ApiActionType::GetLogs,
        ];

        ALL
    }
}

impl From<ApiRequest> for ApiActionType {
    fn from(request: ApiRequest) -> Self {
        match request.request_type {
            ApiRequestType::BlockWithTxs => Self::BlockWithTxs,
            ApiRequestType::Balance => Self::Balance,
            ApiRequestType::GetLogs => Self::GetLogs,
        }
    }
}

/// Generic wrapper of all the actions that can be done in loadtest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionType {
    InitComplete,
    Tx(TxActionType),
    Api(ApiActionType),
    Subscription(SubscriptionType),
}

impl From<TxActionType> for ActionType {
    fn from(action: TxActionType) -> Self {
        Self::Tx(action)
    }
}

impl From<ApiActionType> for ActionType {
    fn from(action: ApiActionType) -> Self {
        Self::Api(action)
    }
}

impl From<TxCommand> for ActionType {
    fn from(command: TxCommand) -> Self {
        Self::Tx(command.command_type.into())
    }
}

impl From<SubscriptionType> for ActionType {
    fn from(subscription_type: SubscriptionType) -> Self {
        Self::Subscription(subscription_type)
    }
}

impl ActionType {
    /// Returns the vector containing the list of all the supported actions.
    /// May be useful in different collectors to initialize their internal states.
    pub fn all() -> Vec<Self> {
        TxActionType::all()
            .iter()
            .copied()
            .map(Self::from)
            .chain(ApiActionType::all().iter().copied().map(Self::from))
            .collect()
    }
}
