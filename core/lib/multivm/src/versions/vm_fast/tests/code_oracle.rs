use crate::{
    versions::testonly::code_oracle::{
        test_bootloader_in_code_oracle, test_bootloader_in_create2, test_code_oracle,
        test_code_oracle_big_bytecode, test_cross_version_alias_in_code_oracle,
        test_refunds_in_code_oracle,
    },
    vm_fast::Vm,
};

#[test]
fn code_oracle() {
    test_code_oracle::<Vm<_>>();
}

#[test]
fn code_oracle_big_bytecode() {
    test_code_oracle_big_bytecode::<Vm<_>>();
}

#[test]
fn refunds_in_code_oracle() {
    test_refunds_in_code_oracle::<Vm<_>>();
}

/// Requires the `zksync_vm2` half of the fix: decommit freshness must be keyed by the
/// header-normalized preimage there as well.
///
/// Ignored until that half exists. `WorldDiff::decommit_opcode` keys `decommitted_hashes` on the
/// versioned hash, so the native and EVM hashes of one bytecode are two entries and the second
/// `CodeOracle` call pays the extra cost again (188 gas here). `vm_latest` is unaffected: zk_evm
/// normalizes to a 28-byte `VersionedHashNormalizedPreimage` before touching the decommit queue.
/// Measured against `vm2` v0.6.2 as well -- the keying is unchanged there, so the bump upstream
/// took in era f3c2f8efd does not lift this.
#[test]
#[ignore = "needs the `zksync_vm2` half of the cross-version decommit alias fix"]
fn cross_version_alias_in_code_oracle() {
    test_cross_version_alias_in_code_oracle::<Vm<_>>();
}

#[test_casing::test_casing(2, [false, true])]
fn bootloader_in_code_oracle(store_bootloader: bool) {
    test_bootloader_in_code_oracle::<Vm<_>>(store_bootloader);
}

#[test]
fn bootloader_in_create2() {
    test_bootloader_in_create2::<Vm<_>>();
}
