// "Rich addressing" opcodes are opcodes that can write their return value/read the input onto the stack
// and so take 1-2 RAM permutations more than an average opcode.
// In the worst case, a rich addressing may take 3 ram permutations
// (1 for reading the opcode, 1 for writing input value, 1 for writing output value).
pub(crate) const RICH_ADDRESSING_OPCODE_RAM_CYCLES: u32 = 3;

pub(crate) const AVERAGE_OPCODE_RAM_CYCLES: u32 = 1;

pub(crate) const STORAGE_READ_RAM_CYCLES: u32 = 1;
pub(crate) const STORAGE_READ_LOG_DEMUXER_CYCLES: u32 = 1;
pub(crate) const STORAGE_READ_STORAGE_SORTER_CYCLES: u32 = 1;
pub(crate) const STORAGE_READ_STORAGE_APPLICATION_CYCLES: u32 = 1;

pub(crate) const EVENT_RAM_CYCLES: u32 = 1;
pub(crate) const EVENT_LOG_DEMUXER_CYCLES: u32 = 2;
pub(crate) const EVENT_EVENTS_SORTER_CYCLES: u32 = 2;

pub(crate) const STORAGE_WRITE_RAM_CYCLES: u32 = 1;
pub(crate) const STORAGE_WRITE_LOG_DEMUXER_CYCLES: u32 = 2;
pub(crate) const STORAGE_WRITE_STORAGE_SORTER_CYCLES: u32 = 2;
pub(crate) const STORAGE_WRITE_STORAGE_APPLICATION_CYCLES: u32 = 2;

pub(crate) const FAR_CALL_RAM_CYCLES: u32 = 1;
pub(crate) const FAR_CALL_STORAGE_SORTER_CYCLES: u32 = 1;
pub(crate) const FAR_CALL_CODE_DECOMMITTER_SORTER_CYCLES: u32 = 1;

// 5 RAM permutations, because: 1 to read opcode + 2 reads + 2 writes.
// 2 reads and 2 writes are needed because unaligned access is implemented with
// aligned queries.
pub(crate) const UMA_WRITE_RAM_CYCLES: u32 = 5;

// 3 RAM permutations, because: 1 to read opcode + 2 reads.
// 2 reads are needed because unaligned access is implemented with aligned queries.
pub(crate) const UMA_READ_RAM_CYCLES: u32 = 3;

pub(crate) const PRECOMPILE_RAM_CYCLES: u32 = 1;
pub(crate) const PRECOMPILE_LOG_DEMUXER_CYCLES: u32 = 1;
