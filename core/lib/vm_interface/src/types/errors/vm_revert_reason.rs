use std::fmt;

use zksync_types::U256;

#[derive(Debug, thiserror::Error)]
pub enum VmRevertReasonParsingError {
    #[error("Incorrect data offset. Data: {0:?}")]
    IncorrectDataOffset(Vec<u8>),
    #[error("Input is too short. Data: {0:?}")]
    InputIsTooShort(Vec<u8>),
    #[error("Incorrect string length. Data: {0:?}")]
    IncorrectStringLength(Vec<u8>),
}

/// Rich Revert Reasons `https://github.com/0xProject/ZEIPs/issues/32`
#[derive(Debug, Clone, PartialEq)]
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
        let data_offset = U256::from_big_endian(&bytes[0..32]).as_usize();

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

        let string_length = U256::from_big_endian(&data[0..32]).as_usize();

        if string_length + 32 > data.len() {
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
    use super::VmRevertReason;

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
        assert!(matches!(reason, VmRevertReason::Unknown { .. }));
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
}
