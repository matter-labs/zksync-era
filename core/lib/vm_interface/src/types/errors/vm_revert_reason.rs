use std::fmt;

use zksync_types::U256;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VmRevertReasonParsingError {
    #[error("Incorrect data offset. Data: {0:?}")]
    IncorrectDataOffset(Vec<u8>),
    #[error("Input is too short. Data: {0:?}")]
    InputIsTooShort(Vec<u8>),
    #[error("Incorrect string length. Data: {0:?}")]
    IncorrectStringLength(Vec<u8>),
}

/// Parse a 32-byte big-endian word as `usize`, returning `None` if it does not
/// fit. `U256::as_usize` panics when the value exceeds `usize::MAX`, so an
/// oversized word in attacker-supplied revert data would otherwise panic
/// mid-parsing instead of yielding a clean parse error (and aborts outright on
/// 32-bit targets such as the RISC-V proving guest).
fn word_to_usize(word: &[u8; 32]) -> Option<usize> {
    let value = U256::from_big_endian(word);
    if value > U256::from(usize::MAX as u64) {
        None
    } else {
        Some(value.as_usize())
    }
}

/// Rich Revert Reasons `https://github.com/0xProject/ZEIPs/issues/32`
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum VmRevertReason {
    General {
        msg: String,
        data: Vec<u8>,
    },
    InnerTxError,
    VmError,
    Unknown {
        function_selector: Vec<u8>,
        data: Vec<u8>,
    },
}

impl VmRevertReason {
    const GENERAL_ERROR_SELECTOR: &'static [u8] = &[0x08, 0xc3, 0x79, 0xa0];

    fn parse_general_error(raw_bytes: &[u8]) -> Result<Self, VmRevertReasonParsingError> {
        let bytes = &raw_bytes[4..];
        if bytes.len() < 32 {
            return Err(VmRevertReasonParsingError::InputIsTooShort(bytes.to_vec()));
        }
        // A word exceeding `usize::MAX` can't be a valid in-bounds offset; map it
        // to the offset error rather than panicking in `as_usize`. The `bytes.len() < 32`
        // guard above makes the fixed-size conversion infallible.
        let offset_word = <&[u8; 32]>::try_from(&bytes[..32]).expect("checked bytes.len() >= 32");
        let data_offset = word_to_usize(offset_word)
            .ok_or_else(|| VmRevertReasonParsingError::IncorrectDataOffset(bytes.to_vec()))?;

        // Data offset couldn't be less than 32 because data offset size is 32 bytes
        // and data offset bytes are part of the offset. Also data offset couldn't be greater than
        // data length
        if data_offset > bytes.len() || data_offset < 32 {
            return Err(VmRevertReasonParsingError::IncorrectDataOffset(
                bytes.to_vec(),
            ));
        };

        let data = &bytes[data_offset..];

        if data.len() < 32 {
            return Err(VmRevertReasonParsingError::InputIsTooShort(bytes.to_vec()));
        };

        // The `data.len() < 32` guard above makes this conversion infallible.
        let length_word = <&[u8; 32]>::try_from(&data[..32]).expect("checked data.len() >= 32");
        let string_length = word_to_usize(length_word)
            .ok_or_else(|| VmRevertReasonParsingError::IncorrectStringLength(bytes.to_vec()))?;

        // `string_length + 32` can itself overflow `usize` (e.g. on 32-bit targets),
        // so add with overflow detection rather than a bare `+`.
        if string_length
            .checked_add(32)
            .is_none_or(|end| end > data.len())
        {
            return Err(VmRevertReasonParsingError::IncorrectStringLength(
                bytes.to_vec(),
            ));
        };

        let raw_data = &data[32..32 + string_length];
        Ok(Self::General {
            msg: String::from_utf8_lossy(raw_data).to_string(),
            data: raw_bytes.to_vec(),
        })
    }

    pub fn to_user_friendly_string(&self) -> String {
        match self {
            // In case of `Unknown` reason we suppress it to prevent verbose `Error function_selector = 0x{}`
            // message shown to user.
            VmRevertReason::Unknown { .. } => "".to_owned(),
            _ => self.to_string(),
        }
    }

    pub fn encoded_data(&self) -> Vec<u8> {
        match self {
            VmRevertReason::Unknown { data, .. } => data.clone(),
            VmRevertReason::General { data, .. } => data.clone(),
            _ => vec![],
        }
    }

    fn try_from_bytes(bytes: &[u8]) -> Result<Self, VmRevertReasonParsingError> {
        if bytes.len() < 4 {
            // Note, that when the method reverts with no data
            // the selector is empty as well.
            // For now, we only accept errors with either no data or
            // the data with complete selectors.
            if !bytes.is_empty() {
                return Err(VmRevertReasonParsingError::IncorrectStringLength(
                    bytes.to_owned(),
                ));
            }

            let result = VmRevertReason::Unknown {
                function_selector: vec![],
                data: bytes.to_vec(),
            };

            return Ok(result);
        }

        let function_selector = &bytes[0..4];
        match function_selector {
            VmRevertReason::GENERAL_ERROR_SELECTOR => Self::parse_general_error(bytes),
            _ => {
                let result = VmRevertReason::Unknown {
                    function_selector: function_selector.to_vec(),
                    data: bytes.to_vec(),
                };
                Ok(result)
            }
        }
    }
}

impl From<&[u8]> for VmRevertReason {
    fn from(error_msg: &[u8]) -> Self {
        match Self::try_from_bytes(error_msg) {
            Ok(reason) => reason,
            Err(_) => {
                let function_selector = if error_msg.len() >= 4 {
                    error_msg[0..4].to_vec()
                } else {
                    error_msg.to_vec()
                };

                let data = if error_msg.len() > 4 {
                    error_msg[4..].to_vec()
                } else {
                    vec![]
                };

                VmRevertReason::Unknown {
                    function_selector,
                    data,
                }
            }
        }
    }
}

impl fmt::Display for VmRevertReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use VmRevertReason::{General, InnerTxError, Unknown, VmError};

        match self {
            General { msg, .. } => write!(f, "{}", msg),
            VmError => write!(f, "VM Error",),
            InnerTxError => write!(f, "Bootloader-based tx failed"),
            Unknown {
                function_selector,
                data,
            } => write!(
                f,
                "Error function_selector = 0x{}, data = 0x{}",
                hex::encode(function_selector),
                hex::encode(data)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::{VmRevertReason, VmRevertReasonParsingError};

    #[test]
    fn revert_reason_parsing() {
        let msg = vec![
            8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 38, 69, 82, 67, 50, 48, 58, 32, 116, 114, 97, 110,
            115, 102, 101, 114, 32, 97, 109, 111, 117, 110, 116, 32, 101, 120, 99, 101, 101, 100,
            115, 32, 98, 97, 108, 97, 110, 99, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let reason = VmRevertReason::try_from_bytes(msg.as_slice()).expect("Shouldn't be error");
        assert_eq!(
            reason,
            VmRevertReason::General {
                msg: "ERC20: transfer amount exceeds balance".to_string(),
                data: msg
            }
        );
    }

    #[test]
    fn revert_reason_with_wrong_function_selector() {
        let msg = vec![
            8, 195, 121, 161, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 38, 69, 82, 67, 50, 48, 58, 32, 116, 114, 97, 110,
            115, 102, 101, 114, 32, 97, 109, 111, 117, 110, 116, 32, 101, 120, 99, 101, 101, 100,
            115, 32, 98, 97, 108, 97, 110, 99, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let reason = VmRevertReason::try_from_bytes(msg.as_slice()).expect("Shouldn't be error");
        assert_matches!(reason, VmRevertReason::Unknown { .. });
    }

    #[test]
    fn revert_reason_with_wrong_data_offset() {
        let msg = vec![
            8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 31, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 38, 69, 82, 67, 50, 48, 58, 32, 116, 114, 97, 110,
            115, 102, 101, 114, 32, 97, 109, 111, 117, 110, 116, 32, 101, 120, 99, 101, 101, 100,
            115, 32, 98, 97, 108, 97, 110, 99, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let reason = VmRevertReason::try_from_bytes(msg.as_slice());
        assert!(reason.is_err());
    }

    #[test]
    fn revert_reason_with_big_data_offset() {
        let msg = vec![
            8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 132, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 38, 69, 82, 67, 50, 48, 58, 32, 116, 114, 97, 110,
            115, 102, 101, 114, 32, 97, 109, 111, 117, 110, 116, 32, 101, 120, 99, 101, 101, 100,
            115, 32, 98, 97, 108, 97, 110, 99, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let reason = VmRevertReason::try_from_bytes(msg.as_slice());
        assert!(reason.is_err());
    }

    #[test]
    fn revert_reason_with_wrong_string_length() {
        let msg = vec![
            8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 158, 69, 82, 67, 50, 48, 58, 32, 116, 114, 97, 110,
            115, 102, 101, 114, 32, 97, 109, 111, 117, 110, 116, 32, 101, 120, 99, 101, 101, 100,
            115, 32, 98, 97, 108, 97, 110, 99, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let reason = VmRevertReason::try_from_bytes(msg.as_slice());
        assert!(reason.is_err());
    }

    /// 32-byte big-endian word from a `u64` (high bytes zero).
    fn word(value: u64) -> [u8; 32] {
        let mut w = [0u8; 32];
        w[24..].copy_from_slice(&value.to_be_bytes());
        w
    }

    /// A word whose value (2^248) exceeds `usize::MAX` on any platform.
    fn oversized_word() -> [u8; 32] {
        let mut w = [0u8; 32];
        w[0] = 1;
        w
    }

    fn general_error(words: &[[u8; 32]], tail: &[u8]) -> Vec<u8> {
        let mut bytes = vec![8, 195, 121, 160]; // general error selector
        for w in words {
            bytes.extend_from_slice(w);
        }
        bytes.extend_from_slice(tail);
        bytes
    }

    #[test]
    fn data_offset_overflow_is_rejected_not_panicked() {
        let raw = general_error(&[oversized_word()], &[]);
        assert_matches!(
            VmRevertReason::try_from_bytes(&raw),
            Err(VmRevertReasonParsingError::IncorrectDataOffset(_))
        );
    }

    #[test]
    fn string_length_overflow_is_rejected_not_panicked() {
        // offset = 32 (valid), then an oversized string-length word.
        let raw = general_error(&[word(32), oversized_word()], &[]);
        assert_matches!(
            VmRevertReason::try_from_bytes(&raw),
            Err(VmRevertReasonParsingError::IncorrectStringLength(_))
        );
    }
}
