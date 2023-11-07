use zksync_basic_types::Address;

/// Priority op should be executed for this number of eth blocks.
pub const PRIORITY_EXPIRATION: u64 = 50000;
pub const MAX_L1_TRANSACTION_GAS_LIMIT: u64 = 300000;
pub static ETHEREUM_ADDRESS: Address = Address::zero();

/// This the number of pubdata such that it should be always possible to publish
/// from a single transaction. Note, that these pubdata bytes include only bytes that are
/// to be published inside the body of transaction (i.e. excluding of factory deps).
pub const GUARANTEED_PUBDATA_PER_L1_BATCH: u64 = 4000;

/// The maximum number of pubdata per L1 batch. This limit is due to the fact that the Ethereum
/// nodes do not accept transactions that have more than 128kb of pubdata.
/// The 18kb margin is left in case of any inpreciseness of the pubdata calculation.
pub const MAX_PUBDATA_PER_L1_BATCH: u64 = 110000;

// TODO: import from zkevm_opcode_defs once VM1.3 is supported
pub const MAX_L2_TX_GAS_LIMIT: u64 = 80000000;

// The users should always be able to provide `MAX_GAS_PER_PUBDATA_BYTE` gas per pubdata in their
// transactions so that they are able to send at least GUARANTEED_PUBDATA_PER_L1_BATCH bytes per
// transaction.
pub const MAX_GAS_PER_PUBDATA_BYTE: u64 = MAX_L2_TX_GAS_LIMIT / GUARANTEED_PUBDATA_PER_L1_BATCH;

// The L1->L2 are required to have the following gas per pubdata byte.
pub const REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE: u64 = 800;
