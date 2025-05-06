use zksync_basic_types::Address;

pub const MAX_L1_TRANSACTION_GAS_LIMIT: u64 = 300000;
pub const ETHEREUM_ADDRESS: Address = Address::zero();

// TODO: import from `zkevm_opcode_defs` once `VM1.3` is supported
pub const MAX_L2_TX_GAS_LIMIT: u64 = 80000000;

// The L1->L2 are required to have the following gas per pubdata byte.
pub const REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE: u64 = 800;

// The default gas per pubdata byte for L2 transactions, that is used, for instance, when we need to
// insert some default value for type 2 transactions.
// It is a realistic value, but it is large enough to fill into any batch regardless of the pubdata price.
pub const DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE: u64 = 50_000;
