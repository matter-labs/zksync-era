use circuit_sequencer_api_1_5_0::{geometry_config::get_geometry_config, toolset::GeometryConfig};
use vm2::{CycleStats, Opcode, OpcodeType, StateInterface, Tracer};
use zksync_vm_interface::CircuitStatistic;

// "Rich addressing" opcodes are opcodes that can write their return value/read the input onto the stack
// and so take 1-2 RAM permutations more than an average opcode.
// In the worst case, a rich addressing may take 3 ram permutations
// (1 for reading the opcode, 1 for writing input value, 1 for writing output value).
pub(crate) const RICH_ADDRESSING_OPCODE_RAM_CYCLES: u32 = 3;

pub(crate) const AVERAGE_OPCODE_RAM_CYCLES: u32 = 1;

pub(crate) const STORAGE_READ_RAM_CYCLES: u32 = 1;
pub(crate) const STORAGE_READ_LOG_DEMUXER_CYCLES: u32 = 1;
pub(crate) const STORAGE_READ_STORAGE_SORTER_CYCLES: u32 = 1;

pub(crate) const TRANSIENT_STORAGE_READ_RAM_CYCLES: u32 = 1;
pub(crate) const TRANSIENT_STORAGE_READ_LOG_DEMUXER_CYCLES: u32 = 1;
pub(crate) const TRANSIENT_STORAGE_READ_TRANSIENT_STORAGE_CHECKER_CYCLES: u32 = 1;

pub(crate) const EVENT_RAM_CYCLES: u32 = 1;
pub(crate) const EVENT_LOG_DEMUXER_CYCLES: u32 = 2;
pub(crate) const EVENT_EVENTS_SORTER_CYCLES: u32 = 2;

pub(crate) const STORAGE_WRITE_RAM_CYCLES: u32 = 1;
pub(crate) const STORAGE_WRITE_LOG_DEMUXER_CYCLES: u32 = 2;
pub(crate) const STORAGE_WRITE_STORAGE_SORTER_CYCLES: u32 = 2;

pub(crate) const TRANSIENT_STORAGE_WRITE_RAM_CYCLES: u32 = 1;
pub(crate) const TRANSIENT_STORAGE_WRITE_LOG_DEMUXER_CYCLES: u32 = 2;
pub(crate) const TRANSIENT_STORAGE_WRITE_TRANSIENT_STORAGE_CHECKER_CYCLES: u32 = 2;

pub(crate) const FAR_CALL_RAM_CYCLES: u32 = 1;
pub(crate) const FAR_CALL_STORAGE_SORTER_CYCLES: u32 = 1;
pub(crate) const FAR_CALL_CODE_DECOMMITTER_SORTER_CYCLES: u32 = 1;
pub(crate) const FAR_CALL_LOG_DEMUXER_CYCLES: u32 = 1;

// 5 RAM permutations, because: 1 to read opcode + 2 reads + 2 writes.
// 2 reads and 2 writes are needed because unaligned access is implemented with
// aligned queries.
pub(crate) const UMA_WRITE_RAM_CYCLES: u32 = 5;

// 3 RAM permutations, because: 1 to read opcode + 2 reads.
// 2 reads are needed because unaligned access is implemented with aligned queries.
pub(crate) const UMA_READ_RAM_CYCLES: u32 = 3;

pub(crate) const PRECOMPILE_RAM_CYCLES: u32 = 1;
pub(crate) const PRECOMPILE_LOG_DEMUXER_CYCLES: u32 = 1;

pub(crate) const LOG_DECOMMIT_RAM_CYCLES: u32 = 1;
pub(crate) const LOG_DECOMMIT_DECOMMITTER_SORTER_CYCLES: u32 = 1;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct CircuitsTracer {
    main_vm_cycles: u32,
    ram_permutation_cycles: u32,
    storage_application_cycles: u32,
    storage_sorter_cycles: u32,
    code_decommitter_cycles: u32,
    code_decommitter_sorter_cycles: u32,
    log_demuxer_cycles: u32,
    events_sorter_cycles: u32,
    keccak256_cycles: u32,
    ecrecover_cycles: u32,
    sha256_cycles: u32,
    secp256k1_verify_cycles: u32,
    transient_storage_checker_cycles: u32,
}

impl Tracer for CircuitsTracer {
    fn after_instruction<OP: OpcodeType, S: StateInterface>(&mut self, _state: &mut S) {
        self.main_vm_cycles += 1;

        match OP::VALUE {
            Opcode::Nop
            | Opcode::Add
            | Opcode::Sub
            | Opcode::Mul
            | Opcode::Div
            | Opcode::Jump
            | Opcode::Xor
            | Opcode::And
            | Opcode::Or
            | Opcode::ShiftLeft
            | Opcode::ShiftRight
            | Opcode::RotateLeft
            | Opcode::RotateRight
            | Opcode::PointerAdd
            | Opcode::PointerSub
            | Opcode::PointerPack
            | Opcode::PointerShrink => {
                self.ram_permutation_cycles += RICH_ADDRESSING_OPCODE_RAM_CYCLES;
            }
            Opcode::This
            | Opcode::Caller
            | Opcode::CodeAddress
            | Opcode::ContextMeta
            | Opcode::ErgsLeft
            | Opcode::SP
            | Opcode::ContextU128
            | Opcode::SetContextU128
            | Opcode::AuxMutating0
            | Opcode::IncrementTxNumber
            | Opcode::Ret(_)
            | Opcode::NearCall => {
                self.ram_permutation_cycles += AVERAGE_OPCODE_RAM_CYCLES;
            }
            Opcode::StorageRead => {
                self.ram_permutation_cycles += STORAGE_READ_RAM_CYCLES;
                self.log_demuxer_cycles += STORAGE_READ_LOG_DEMUXER_CYCLES;
                self.storage_sorter_cycles += STORAGE_READ_STORAGE_SORTER_CYCLES;
            }
            Opcode::TransientStorageRead => {
                self.ram_permutation_cycles += TRANSIENT_STORAGE_READ_RAM_CYCLES;
                self.log_demuxer_cycles += TRANSIENT_STORAGE_READ_LOG_DEMUXER_CYCLES;
                self.transient_storage_checker_cycles +=
                    TRANSIENT_STORAGE_READ_TRANSIENT_STORAGE_CHECKER_CYCLES;
            }
            Opcode::StorageWrite => {
                self.ram_permutation_cycles += STORAGE_WRITE_RAM_CYCLES;
                self.log_demuxer_cycles += STORAGE_WRITE_LOG_DEMUXER_CYCLES;
                self.storage_sorter_cycles += STORAGE_WRITE_STORAGE_SORTER_CYCLES;
            }
            Opcode::TransientStorageWrite => {
                self.ram_permutation_cycles += TRANSIENT_STORAGE_WRITE_RAM_CYCLES;
                self.log_demuxer_cycles += TRANSIENT_STORAGE_WRITE_LOG_DEMUXER_CYCLES;
                self.transient_storage_checker_cycles +=
                    TRANSIENT_STORAGE_WRITE_TRANSIENT_STORAGE_CHECKER_CYCLES;
            }
            Opcode::L2ToL1Message | Opcode::Event => {
                self.ram_permutation_cycles += EVENT_RAM_CYCLES;
                self.log_demuxer_cycles += EVENT_LOG_DEMUXER_CYCLES;
                self.events_sorter_cycles += EVENT_EVENTS_SORTER_CYCLES;
            }
            Opcode::PrecompileCall => {
                self.ram_permutation_cycles += PRECOMPILE_RAM_CYCLES;
                self.log_demuxer_cycles += PRECOMPILE_LOG_DEMUXER_CYCLES;
            }
            Opcode::Decommit => {
                // Note, that for decommit the log demuxer circuit is not used.
                self.ram_permutation_cycles += LOG_DECOMMIT_RAM_CYCLES;
                self.code_decommitter_sorter_cycles += LOG_DECOMMIT_DECOMMITTER_SORTER_CYCLES;
            }
            Opcode::FarCall(_) => {
                self.ram_permutation_cycles += FAR_CALL_RAM_CYCLES;
                self.code_decommitter_sorter_cycles += FAR_CALL_CODE_DECOMMITTER_SORTER_CYCLES;
                self.storage_sorter_cycles += FAR_CALL_STORAGE_SORTER_CYCLES;
                self.log_demuxer_cycles += FAR_CALL_LOG_DEMUXER_CYCLES;
            }
            Opcode::AuxHeapWrite | Opcode::HeapWrite /* StaticMemoryWrite */ => {
                self.ram_permutation_cycles += UMA_WRITE_RAM_CYCLES;
            }
            Opcode::AuxHeapRead | Opcode::HeapRead | Opcode::PointerRead /* StaticMemoryRead */ => {
                self.ram_permutation_cycles += UMA_READ_RAM_CYCLES;
            }
        }
    }

    fn on_extra_prover_cycles(&mut self, stats: CycleStats) {
        match stats {
            CycleStats::Keccak256(cycles) => self.keccak256_cycles += cycles,
            CycleStats::Sha256(cycles) => self.sha256_cycles += cycles,
            CycleStats::EcRecover(cycles) => self.ecrecover_cycles += cycles,
            CycleStats::Secp256k1Verify(cycles) => self.secp256k1_verify_cycles += cycles,
            CycleStats::Decommit(cycles) => self.code_decommitter_cycles += cycles,
            CycleStats::StorageRead => self.storage_application_cycles += 1,
            CycleStats::StorageWrite => self.storage_application_cycles += 2,
        }
    }
}

impl CircuitsTracer {
    pub(crate) fn circuit_statistic(&self) -> CircuitStatistic {
        CircuitStatistic {
            main_vm: self.main_vm_cycles as f32 / GEOMETRY_CONFIG.cycles_per_vm_snapshot as f32,
            ram_permutation: self.ram_permutation_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_ram_permutation as f32,
            storage_application: self.storage_application_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_storage_application as f32,
            storage_sorter: self.storage_sorter_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_storage_sorter as f32,
            code_decommitter: self.code_decommitter_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_code_decommitter as f32,
            code_decommitter_sorter: self.code_decommitter_sorter_cycles as f32
                / GEOMETRY_CONFIG.cycles_code_decommitter_sorter as f32,
            log_demuxer: self.log_demuxer_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_log_demuxer as f32,
            events_sorter: self.events_sorter_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_events_or_l1_messages_sorter as f32,
            keccak256: self.keccak256_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_keccak256_circuit as f32,
            ecrecover: self.ecrecover_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_ecrecover_circuit as f32,
            sha256: self.sha256_cycles as f32 / GEOMETRY_CONFIG.cycles_per_sha256_circuit as f32,
            secp256k1_verify: self.secp256k1_verify_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_secp256r1_verify_circuit as f32,
            transient_storage_checker: self.transient_storage_checker_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_transient_storage_sorter as f32,
        }
    }
}

const GEOMETRY_CONFIG: GeometryConfig = get_geometry_config();
