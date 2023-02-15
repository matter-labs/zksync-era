#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageStateRecord {
    pub address: Vec<u8>,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}
