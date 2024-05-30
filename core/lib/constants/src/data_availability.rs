/// An interval with which the dispatcher is polling the DA layer for the inclusion of the blobs.
pub const DEFAULT_POLLING_INTERVAL: u32 = 5;
/// The maximum number of rows that the dispatcher is fetching from the database.
pub const DEFAULT_QUERY_ROWS_LIMIT: u32 = 100;
/// The maximum number of retries for the dispatching of a blob.
pub const DEFAULT_MAX_RETRIES: u16 = 5;
