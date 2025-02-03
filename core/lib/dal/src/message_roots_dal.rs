use std::{convert::TryInto, result};

use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{message_root::MessageRoot, L1BatchNumber, SLChainId, H256};

use crate::Core;

#[derive(Debug)]
pub struct MessageRootDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl MessageRootDal<'_, '_> {
    pub async fn save_message_root(&mut self, msg_root: MessageRoot) -> DalResult<()> {
        Ok(())
    }

    pub async fn set_message_root(
        &mut self,
        chain_id: SLChainId,
        number: L1BatchNumber,
        message_root: H256,
        // proof: BatchAndChainMerklePath,
    ) -> DalResult<()> {
        println!(
            "set_message_root {:?} {:?} {:?}",
            chain_id.0, number.0, message_root
        );
        sqlx::query!(
            r#"
            INSERT INTO message_roots (chain_id, block_number, message_root_hash)
            VALUES ($1, $2, $3)
            ON CONFLICT DO NOTHING
            "#,
            chain_id.0 as i64,
            number.0 as i64,
            message_root.as_bytes()
        )
        .instrument("set_message_root")
        .with_arg("chain_id", &chain_id)
        .with_arg("number", &number)
        .with_arg("message_root", &message_root)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_latest_message_root(&mut self) -> DalResult<Option<MessageRoot>> {
        let result: Vec<MessageRoot> = sqlx::query!(
            r#"
            FROM message_roots
            ORDER BY block_number DESC
            LIMIT 1
            "#
        )
        .instrument("get_latest_message_root")
        .fetch_optional(self.storage)
        .await?
        .into_iter()
        .map(|record| {
            let block_number = record.block_number as u32;
            let root = H256::from_slice(&record.message_root_hash);
            let chain_id = record.chain_id as u32;
            MessageRoot::new(chain_id, block_number, root)
        })
        .collect();
        println!("get_latest_message_root {:?}", result);

        if result.is_empty() {
            return Ok(None);
        }
        Ok(Some(result[0]))
        // let result = result.unwrap();
        // Ok(Some(MessageRoot::new(result.unwrap().clone().chain_id as u32, result.unwrap().clone().block_number as u32, H256::from_slice(&result.unwrap().clone().message_root_hash))))
    }
}
