use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::Address;

use crate::Core;

#[derive(Debug)]
pub struct ContractDeployAllowListDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl ContractDeployAllowListDal<'_, '_> {
    /// Checks if a given address exists in `contract_deploy_allow_list`
    pub async fn is_address_allowed(&mut self, address: &Address) -> DalResult<bool> {
        let exists = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM contract_deploy_allow_list WHERE address = $1
            )
            "#,
            address.as_bytes()
        )
        .instrument("is_address_allowed")
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(exists.unwrap_or(false))
    }
}
