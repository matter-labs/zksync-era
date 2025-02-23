use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::InstrumentExt,
};
use zksync_types::Address;

use crate::Core;

#[derive(Debug)]
pub struct ContractDeployAllowListDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl ContractDeployAllowListDal<'_, '_> {
    
    /// Returns every address in `contract_deploy_allow_list`, using a prepared statement
    pub async fn get_allow_list(&mut self) -> DalResult<Vec<Address>> {
        let rows = sqlx::query!(
            r#"
            SELECT address
            FROM contract_deploy_allow_list
            "#
        )
        .instrument("get_allow_list")    // Tag the query for logs / metrics
        .report_latency()                // Measure query latency
        .fetch_all(self.storage)
        .await?;


        Ok(rows
            .into_iter()
            .map(|row| Address::from_slice(&row.address))
            .collect())
    }
}
