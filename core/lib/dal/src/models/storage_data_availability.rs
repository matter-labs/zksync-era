use chrono::NaiveDateTime;

#[derive(Debug, Clone)]
pub struct StorageDataAvailability {
    pub l1_batch_number: i64,
    pub blob_id: Option<Vec<u8>>,
    pub inclusion_data: Option<Vec<u8>>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
