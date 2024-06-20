use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
};

use anyhow::{anyhow, Context, Result};
use secp256k1::PublicKey;

// Gramine-specific device file from which the attestation quote can be read
pub(crate) const GRAMINE_ATTESTATION_QUOTE_DEVICE_FILE: &str = "/dev/attestation/quote";

// Gramine-specific device file from which the attestation type can be read
// pub(crate) const GRAMINE_ATTESTATION_TYPE_DEVICE_FILE: &str = "/dev/attestation/attestation_type";

// Gramine-specific device file. User data should be written to this file before obtaining the
// attestation quote
pub(crate) const GRAMINE_ATTESTATION_USER_REPORT_DATA_DEVICE_FILE: &str =
    "/dev/attestation/user_report_data";

pub fn save_attestation_user_report_data(pubkey: PublicKey) -> Result<()> {
    let mut user_report_data_file = OpenOptions::new()
        .write(true)
        .open(GRAMINE_ATTESTATION_USER_REPORT_DATA_DEVICE_FILE)
        .context(format!(
            "Failed to open {} for writing user report data",
            GRAMINE_ATTESTATION_USER_REPORT_DATA_DEVICE_FILE
        ))?;
    user_report_data_file
        .write_all(&pubkey.serialize())
        .map_err(|err| anyhow!("Failed to save user report data: {err}"))
}

pub fn get_attestation_quote() -> Result<Vec<u8>> {
    let mut quote_file = File::open(GRAMINE_ATTESTATION_QUOTE_DEVICE_FILE).context(format!(
        "Failed to open {} for reading SGX quote",
        GRAMINE_ATTESTATION_QUOTE_DEVICE_FILE
    ))?;
    let mut quote = Vec::new();
    quote_file.read_to_end(&mut quote)?;
    Ok(quote)
}
