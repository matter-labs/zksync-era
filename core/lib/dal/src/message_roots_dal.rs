use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{h256_to_u256, message_root::MessageRoot, L1BatchNumber, SLChainId, H256};

use crate::Core;

#[derive(Debug)]
pub struct MessageRootDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl MessageRootDal<'_, '_> {
    pub async fn save_message_root(&mut self, _msg_root: MessageRoot) -> DalResult<()> {
        Ok(())
    }

    pub async fn set_message_root(
        &mut self,
        chain_id: SLChainId,
        number: L1BatchNumber,
        message_root: &[H256],
        // proof: BatchAndChainMerklePath,
    ) -> DalResult<()> {
        println!(
            "set_message_root {:?} {:?} {:?}",
            chain_id.0, number.0, message_root
        );
        let sides = message_root
            .iter()
            .map(|root| root.as_bytes().to_vec())
            .collect::<Vec<_>>();
        sqlx::query!(
            r#"
            INSERT INTO message_roots (chain_id, block_number, message_root_sides)
            VALUES ($1, $2, $3)
            ON CONFLICT (chain_id, block_number)
            DO UPDATE SET message_root_sides = excluded.message_root_sides;
            "#,
            chain_id.0 as i64,
            i64::from(number.0),
            &sides
        )
        .instrument("set_message_root")
        .with_arg("chain_id", &chain_id)
        .with_arg("number", &number)
        .with_arg("message_root", &message_root)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_latest_message_root(&mut self) -> DalResult<Option<Vec<MessageRoot>>> {
        // kl todo currently this is very inefficient, we insert all the message roots multiple times.
        // At least record which ones we have inserted already.
        let result: Vec<MessageRoot> = sqlx::query!(
            r#"
            WITH Ranked AS (
                SELECT
                    Message_Root_Sides,
                    Chain_Id,
                    Block_Number,
                    ROW_NUMBER()
                        OVER (PARTITION BY Chain_Id ORDER BY Block_Number DESC)
                    AS Rn
                FROM Message_Roots
            )
            
            SELECT Message_Root_Sides, Chain_Id, Block_Number
            FROM Ranked
            WHERE Rn <= 5
            ORDER BY Chain_Id, Block_Number DESC;
            "#
        )
        .instrument("get_latest_message_root")
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|record| {
            let block_number = record.block_number as u32;
            let root = record
                .message_root_sides
                .iter()
                .map(|side| h256_to_u256(H256::from_slice(side)))
                .collect::<Vec<_>>();
            let chain_id = record.chain_id as u32;
            MessageRoot::new(chain_id, block_number, root)
        })
        .collect();

        if result.is_empty() {
            return Ok(None);
        }
        println!("get_latest_message_root {:?}", result);
        Ok(Some(result))
        // let result = result.unwrap();
        // Ok(Some(MessageRoot::new(result.unwrap().clone().chain_id as u32, result.unwrap().clone().block_number as u32, H256::from_slice(&result.unwrap().clone().message_root_sides))))
    }
}
