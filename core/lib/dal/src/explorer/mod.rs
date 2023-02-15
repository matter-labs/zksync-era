use crate::StorageProcessor;
use contract_verification_dal::ContractVerificationDal;
use explorer_accounts_dal::ExplorerAccountsDal;
use explorer_blocks_dal::ExplorerBlocksDal;
use explorer_events_dal::ExplorerEventsDal;
use explorer_misc_dal::ExplorerMiscDal;
use explorer_transactions_dal::ExplorerTransactionsDal;

pub mod contract_verification_dal;
pub mod explorer_accounts_dal;
pub mod explorer_blocks_dal;
pub mod explorer_events_dal;
pub mod explorer_misc_dal;
pub mod explorer_transactions_dal;
pub mod storage_contract_info;

#[derive(Debug)]
pub struct ExplorerIntermediator<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl<'a, 'c> ExplorerIntermediator<'a, 'c> {
    pub fn contract_verification_dal(self) -> ContractVerificationDal<'a, 'c> {
        ContractVerificationDal {
            storage: self.storage,
        }
    }

    pub fn transactions_dal(self) -> ExplorerTransactionsDal<'a, 'c> {
        ExplorerTransactionsDal {
            storage: self.storage,
        }
    }

    pub fn blocks_dal(self) -> ExplorerBlocksDal<'a, 'c> {
        ExplorerBlocksDal {
            storage: self.storage,
        }
    }

    pub fn accounts_dal(self) -> ExplorerAccountsDal<'a, 'c> {
        ExplorerAccountsDal {
            storage: self.storage,
        }
    }

    pub fn misc_dal(self) -> ExplorerMiscDal<'a, 'c> {
        ExplorerMiscDal {
            storage: self.storage,
        }
    }

    pub fn events_dal(self) -> ExplorerEventsDal<'a, 'c> {
        ExplorerEventsDal {
            storage: self.storage,
        }
    }
}
