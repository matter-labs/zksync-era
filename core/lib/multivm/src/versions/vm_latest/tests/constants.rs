/// Some of the constants of the system are implicitly calculated, but they may affect the code and so
/// we added additional checks on them to keep any unwanted changes of those apparent.
#[test]
fn test_that_bootloader_encoding_space_is_large_enoguh() {
    let encoding_space = crate::vm_latest::constants::get_bootloader_tx_encoding_space(
        crate::vm_latest::MultiVmSubversion::latest(),
    );
    assert!(encoding_space >= 325000, "Bootloader tx space is too small");
}
