use zksync_vm2::interface::{
    CycleStats, GlobalStateInterface, Opcode, OpcodeType, ShouldStop, Tracer,
};

use crate::interface::{FeatureId, FeatureVector};

/// Counts the Airbender cycle-estimator features of a fast-VM execution, so a batch
/// sequenced here is priced by the same cost table as one sequenced on the legacy VM.
/// Observe-only: a batch runs identically with or without it.
///
/// The mapping must match the legacy-VM tracer in `crate::tracers::cycle_estimator`; a
/// feature no tracer fills is indistinguishable from work the batch never did.
///
/// One field per axis, assembled into a [`FeatureVector`] at the end: `OP::VALUE` is a
/// constant, so an instruction costs a single add on a fixed offset. Updating the map per
/// instruction is materially slower.
#[derive(Debug, Default, Clone)]
pub(super) struct CycleFeatureTracer {
    arith_cheap_op: u64,
    arith_shift_op: u64,
    arith_mul_op: u64,
    arith_div_op: u64,
    arith_ptr_op: u64,
    average_op: u64,
    near_call_count: u64,
    storage_read: u64,
    storage_write: u64,
    transient_storage_read: u64,
    transient_storage_write: u64,
    event: u64,
    precompile_call: u64,
    decommit: u64,
    decommit_repeat: u64,
    far_call: u64,
    uma_write: u64,
    uma_read: u64,
    keccak256_cycles: u64,
    sha256_cycles: u64,
    ec_recover_cycles: u64,
    secp256r1_verify_cycles: u64,
    mod_exp_cycles: u64,
    ec_add_cycles: u64,
    ec_mul_cycles: u64,
    ec_pairing_cycles: u64,
    decommit_cycles: u64,
    storage_application: u64,
    /// Set when the executing instruction emitted `CycleStats::Decommit`, which vm2 emits
    /// only for a fresh decommit. Cleared in `before_instruction` of a DECOMMIT and read
    /// in its `after_instruction`, so it reports that instruction's own decommit — far
    /// calls decommit too, and their flag must not be read as a DECOMMIT's.
    fresh_decommit_seen: bool,
}

impl CycleFeatureTracer {
    pub(super) fn features(&self) -> FeatureVector {
        let mut features = FeatureVector::default();
        for (id, count) in [
            (FeatureId::ArithCheapOp, self.arith_cheap_op),
            (FeatureId::ArithShiftOp, self.arith_shift_op),
            (FeatureId::ArithMulOp, self.arith_mul_op),
            (FeatureId::ArithDivOp, self.arith_div_op),
            (FeatureId::ArithPtrOp, self.arith_ptr_op),
            (FeatureId::AverageOp, self.average_op),
            (FeatureId::NearCallCount, self.near_call_count),
            (FeatureId::StorageRead, self.storage_read),
            (FeatureId::StorageWrite, self.storage_write),
            (FeatureId::TransientStorageRead, self.transient_storage_read),
            (
                FeatureId::TransientStorageWrite,
                self.transient_storage_write,
            ),
            (FeatureId::Event, self.event),
            (FeatureId::PrecompileCall, self.precompile_call),
            (FeatureId::Decommit, self.decommit),
            (FeatureId::DecommitRepeat, self.decommit_repeat),
            (FeatureId::FarCall, self.far_call),
            (FeatureId::UmaWrite, self.uma_write),
            (FeatureId::UmaRead, self.uma_read),
            (FeatureId::Keccak256Cycles, self.keccak256_cycles),
            (FeatureId::Sha256Cycles, self.sha256_cycles),
            (FeatureId::EcRecoverCycles, self.ec_recover_cycles),
            (
                FeatureId::Secp256r1VerifyCycles,
                self.secp256r1_verify_cycles,
            ),
            (FeatureId::ModExpCycles, self.mod_exp_cycles),
            (FeatureId::EcAddCycles, self.ec_add_cycles),
            (FeatureId::EcMulCycles, self.ec_mul_cycles),
            (FeatureId::EcPairingCycles, self.ec_pairing_cycles),
            (FeatureId::DecommitCycles, self.decommit_cycles),
            (FeatureId::StorageApplication, self.storage_application),
        ] {
            if count > 0 {
                features.add(id, count);
            }
        }
        features
    }
}

impl Tracer for CycleFeatureTracer {
    fn before_instruction<OP: OpcodeType, S: GlobalStateInterface>(&mut self, _state: &mut S) {
        // `OP::VALUE` is a constant, so this compiles away for every opcode but DECOMMIT.
        if matches!(OP::VALUE, Opcode::Decommit) {
            self.fresh_decommit_seen = false;
        }
    }

    fn after_instruction<OP: OpcodeType, S: GlobalStateInterface>(
        &mut self,
        _state: &mut S,
    ) -> ShouldStop {
        let counter = match OP::VALUE {
            Opcode::Nop
            | Opcode::Add
            | Opcode::Sub
            | Opcode::Jump
            | Opcode::Xor
            | Opcode::And
            | Opcode::Or => &mut self.arith_cheap_op,
            Opcode::ShiftLeft | Opcode::ShiftRight | Opcode::RotateLeft | Opcode::RotateRight => {
                &mut self.arith_shift_op
            }
            Opcode::Mul => &mut self.arith_mul_op,
            Opcode::Div => &mut self.arith_div_op,
            Opcode::PointerAdd
            | Opcode::PointerSub
            | Opcode::PointerPack
            | Opcode::PointerShrink => &mut self.arith_ptr_op,
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
            | Opcode::Ret(_) => &mut self.average_op,
            // Counted on both axes: a near call costs `average_op` (the same dispatch as
            // a context op) plus `near_call_count` (the frame push/pop).
            Opcode::NearCall => {
                self.near_call_count += 1;
                &mut self.average_op
            }
            Opcode::StorageRead => &mut self.storage_read,
            Opcode::StorageWrite => &mut self.storage_write,
            Opcode::TransientStorageRead => &mut self.transient_storage_read,
            Opcode::TransientStorageWrite => &mut self.transient_storage_write,
            Opcode::L2ToL1Message | Opcode::Event => &mut self.event,
            Opcode::PrecompileCall => &mut self.precompile_call,
            Opcode::Decommit => {
                // `on_extra_prover_cycles` runs inside the opcode handler, so no stat by
                // now means this DECOMMIT repeated a hash from earlier in the run — O(len)
                // work no counter reports.
                if !self.fresh_decommit_seen {
                    self.decommit_repeat += 1;
                }
                &mut self.decommit
            }
            Opcode::FarCall(_) => &mut self.far_call,
            Opcode::AuxHeapWrite | Opcode::HeapWrite | Opcode::StaticMemoryWrite => {
                &mut self.uma_write
            }
            Opcode::AuxHeapRead
            | Opcode::HeapRead
            | Opcode::PointerRead
            | Opcode::StaticMemoryRead => &mut self.uma_read,
        };
        *counter += 1;
        ShouldStop::Continue
    }

    fn on_extra_prover_cycles(&mut self, stats: CycleStats) {
        // The payload is the operation's complexity (hashing rounds / circuit cycles),
        // the unit the cost table prices these axes in.
        match stats {
            CycleStats::Keccak256(c) => self.keccak256_cycles += u64::from(c),
            CycleStats::Sha256(c) => self.sha256_cycles += u64::from(c),
            CycleStats::EcRecover(c) => self.ec_recover_cycles += u64::from(c),
            CycleStats::Secp256r1Verify(c) => self.secp256r1_verify_cycles += u64::from(c),
            CycleStats::Decommit(c) => {
                self.fresh_decommit_seen = true;
                self.decommit_cycles += u64::from(c);
            }
            CycleStats::StorageRead => self.storage_application += 1,
            CycleStats::StorageWrite => self.storage_application += 2,
            CycleStats::EcAdd(c) => self.ec_add_cycles += u64::from(c),
            CycleStats::ModExp(c) => self.mod_exp_cycles += u64::from(c),
            CycleStats::EcMul(c) => self.ec_mul_cycles += u64::from(c),
            CycleStats::EcPairing(c) => self.ec_pairing_cycles += u64::from(c),
        }
    }
}
