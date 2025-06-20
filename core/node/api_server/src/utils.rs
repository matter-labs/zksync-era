//! Utils specific to the API server.

use zksync_dal::{Connection, Core, DalError};
use zksync_web3_decl::error::Web3Error;

/// Opens a readonly transaction over the specified connection.
pub(crate) async fn open_readonly_transaction<'r>(
    conn: &'r mut Connection<'_, Core>,
) -> Result<Connection<'r, Core>, Web3Error> {
    let builder = conn.transaction_builder().map_err(DalError::generalize)?;
    Ok(builder
        .set_readonly()
        .build()
        .await
        .map_err(DalError::generalize)?)
}
