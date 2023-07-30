use crate::{SqlxError, StorageProcessor};
use num::{rational::Ratio, BigUint};
use zksync_utils::{big_decimal_to_ratio, ratio_to_big_decimal};

pub(crate) const COEFFICIENT_PRECISION: usize = 10;

#[derive(Debug)]
pub struct OracleDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl OracleDal<'_, '_> {
    pub async fn update_adjust_coefficient(&mut self, adjust_coefficient: &Ratio<BigUint>) {
        sqlx::query!(
            "INSERT INTO oracle VALUES (1, $1, now()) \
             ON CONFLICT (id) \
             DO UPDATE SET gas_token_adjust_coefficient = $1, gas_token_adjust_coefficient_updated_at = now()",
            ratio_to_big_decimal(adjust_coefficient, COEFFICIENT_PRECISION)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_adjust_coefficient(&mut self) -> Result<Ratio<BigUint>, SqlxError> {
        let oracle = sqlx::query!("SELECT gas_token_adjust_coefficient FROM oracle")
            .fetch_one(self.storage.conn())
            .await?;

        Ok(big_decimal_to_ratio(&oracle.gas_token_adjust_coefficient).unwrap())
    }
}
