use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zksync_system_constants::BOOTLOADER_ADDRESS;

use crate::{zk_evm_types::FarCallOpcode, Address, U256};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum CallType {
    #[serde(serialize_with = "far_call_type_to_u8")]
    #[serde(deserialize_with = "far_call_type_from_u8")]
    Call(FarCallOpcode),
    Create,
    NearCall,
}

/// Represents a call in the VM trace.
/// This version of the call represents the call structure before the 1.5.0 protocol version, where
/// all the gas-related fields were represented as `u32` instead of `u64`.
#[derive(Clone, Serialize, Deserialize)]
pub struct LegacyCall {
    /// Type of the call.
    pub r#type: CallType,
    /// Address of the caller.
    pub from: Address,
    /// Address of the callee.
    pub to: Address,
    /// Gas from the parent call.
    pub parent_gas: u32,
    /// Gas provided for the call.
    pub gas: u32,
    /// Gas used by the call.
    pub gas_used: u32,
    /// Value transferred.
    pub value: U256,
    /// Input data.
    pub input: Vec<u8>,
    /// Output data.
    pub output: Vec<u8>,
    /// Error message provided by vm or some unexpected errors.
    pub error: Option<String>,
    /// Revert reason.
    pub revert_reason: Option<String>,
    /// Subcalls.
    pub calls: Vec<LegacyCall>,
}

/// Represents a call in the VM trace.
/// This version has subcalls in the form of "new" calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyMixedCall {
    /// Type of the call.
    pub r#type: CallType,
    /// Address of the caller.
    pub from: Address,
    /// Address of the callee.
    pub to: Address,
    /// Gas from the parent call.
    pub parent_gas: u32,
    /// Gas provided for the call.
    pub gas: u32,
    /// Gas used by the call.
    pub gas_used: u32,
    /// Value transferred.
    pub value: U256,
    /// Input data.
    pub input: Vec<u8>,
    /// Output data.
    pub output: Vec<u8>,
    /// Error message provided by vm or some unexpected errors.
    pub error: Option<String>,
    /// Revert reason.
    pub revert_reason: Option<String>,
    /// Subcalls.
    pub calls: Vec<Call>,
}

/// Represents a call in the VM trace.
#[derive(Clone, Serialize, Deserialize)]
pub struct Call {
    /// Type of the call.
    pub r#type: CallType,
    /// Address of the caller.
    pub from: Address,
    /// Address of the callee.
    pub to: Address,
    /// Gas from the parent call.
    pub parent_gas: u64,
    /// Gas provided for the call.
    pub gas: u64,
    /// Gas used by the call.
    pub gas_used: u64,
    /// Value transferred.
    pub value: U256,
    /// Input data.
    pub input: Vec<u8>,
    /// Output data.
    pub output: Vec<u8>,
    /// Error message provided by vm or some unexpected errors.
    pub error: Option<String>,
    /// Revert reason.
    pub revert_reason: Option<String>,
    /// Subcalls.
    pub calls: Vec<Call>,
}

impl From<LegacyCall> for Call {
    fn from(legacy_call: LegacyCall) -> Self {
        Self {
            r#type: legacy_call.r#type,
            from: legacy_call.from,
            to: legacy_call.to,
            parent_gas: legacy_call.parent_gas as u64,
            gas: legacy_call.gas as u64,
            gas_used: legacy_call.gas_used as u64,
            value: legacy_call.value,
            input: legacy_call.input,
            output: legacy_call.output,
            error: legacy_call.error,
            revert_reason: legacy_call.revert_reason,
            calls: legacy_call.calls.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<LegacyMixedCall> for Call {
    fn from(legacy_call: LegacyMixedCall) -> Self {
        Self {
            r#type: legacy_call.r#type,
            from: legacy_call.from,
            to: legacy_call.to,
            parent_gas: legacy_call.parent_gas as u64,
            gas: legacy_call.gas as u64,
            gas_used: legacy_call.gas_used as u64,
            value: legacy_call.value,
            input: legacy_call.input,
            output: legacy_call.output,
            error: legacy_call.error,
            revert_reason: legacy_call.revert_reason,
            calls: legacy_call.calls,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LegacyCallConversionOverflowError;

impl TryFrom<Call> for LegacyCall {
    type Error = LegacyCallConversionOverflowError;

    fn try_from(call: Call) -> Result<Self, LegacyCallConversionOverflowError> {
        let calls: Result<Vec<LegacyCall>, LegacyCallConversionOverflowError> =
            call.calls.into_iter().map(LegacyCall::try_from).collect();
        Ok(Self {
            r#type: call.r#type,
            from: call.from,
            to: call.to,
            parent_gas: call
                .parent_gas
                .try_into()
                .map_err(|_| LegacyCallConversionOverflowError)?,
            gas: call
                .gas
                .try_into()
                .map_err(|_| LegacyCallConversionOverflowError)?,
            gas_used: call
                .gas_used
                .try_into()
                .map_err(|_| LegacyCallConversionOverflowError)?,
            value: call.value,
            input: call.input,
            output: call.output,
            error: call.error,
            revert_reason: call.revert_reason,
            calls: calls?,
        })
    }
}

impl TryFrom<Call> for LegacyMixedCall {
    type Error = LegacyCallConversionOverflowError;

    fn try_from(call: Call) -> Result<Self, LegacyCallConversionOverflowError> {
        Ok(Self {
            r#type: call.r#type,
            from: call.from,
            to: call.to,
            parent_gas: call
                .parent_gas
                .try_into()
                .map_err(|_| LegacyCallConversionOverflowError)?,
            gas: call
                .gas
                .try_into()
                .map_err(|_| LegacyCallConversionOverflowError)?,
            gas_used: call
                .gas_used
                .try_into()
                .map_err(|_| LegacyCallConversionOverflowError)?,
            value: call.value,
            input: call.input,
            output: call.output,
            error: call.error,
            revert_reason: call.revert_reason,
            calls: call.calls,
        })
    }
}

impl Call {
    pub fn new_high_level(
        gas: u64,
        gas_used: u64,
        value: U256,
        input: Vec<u8>,
        output: Vec<u8>,
        revert_reason: Option<String>,
        calls: Vec<Call>,
    ) -> Self {
        Self {
            r#type: CallType::Call(FarCallOpcode::Normal),
            from: Address::zero(),
            to: BOOTLOADER_ADDRESS,
            parent_gas: gas,
            gas,
            gas_used,
            value,
            input,
            output,
            error: None,
            revert_reason,
            calls,
        }
    }
}

impl PartialEq for Call {
    fn eq(&self, other: &Self) -> bool {
        self.revert_reason == other.revert_reason
            && self.input == other.input
            && self.from == other.from
            && self.to == other.to
            && self.r#type == other.r#type
            && self.value == other.value
            && self.error == other.error
            && self.output == other.output
            && self.calls == other.calls
    }
}

fn far_call_type_from_u8<'de, D>(deserializer: D) -> Result<FarCallOpcode, D::Error>
where
    D: Deserializer<'de>,
{
    let res = u8::deserialize(deserializer)?;
    match res {
        0 => Ok(FarCallOpcode::Normal),
        1 => Ok(FarCallOpcode::Delegate),
        2 => Ok(FarCallOpcode::Mimic),
        _ => Err(serde::de::Error::custom("Invalid FarCallOpcode")),
    }
}

fn far_call_type_to_u8<S>(far_call_type: &FarCallOpcode, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_u8(*far_call_type as u8)
}

impl Default for Call {
    fn default() -> Self {
        Self {
            r#type: CallType::Call(FarCallOpcode::Normal),
            from: Default::default(),
            to: Default::default(),
            parent_gas: 0,
            gas: 0,
            gas_used: 0,
            value: Default::default(),
            input: vec![],
            output: vec![],
            error: None,
            revert_reason: None,
            calls: vec![],
        }
    }
}

impl fmt::Debug for Call {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Call")
            .field("type", &self.r#type)
            .field("to", &self.to)
            .field("from", &self.from)
            .field("parent_gas", &self.parent_gas)
            .field("gas_used", &self.gas_used)
            .field("gas", &self.gas)
            .field("value", &self.value)
            .field("input", &format_args!("{:?}", self.input))
            .field("output", &format_args!("{:?}", self.output))
            .field("error", &self.error)
            .field("revert_reason", &format_args!("{:?}", self.revert_reason))
            .field("call_traces", &self.calls)
            .finish()
    }
}

impl fmt::Debug for LegacyCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyCall")
            .field("type", &self.r#type)
            .field("to", &self.to)
            .field("from", &self.from)
            .field("parent_gas", &self.parent_gas)
            .field("gas_used", &self.gas_used)
            .field("gas", &self.gas)
            .field("value", &self.value)
            .field("input", &format_args!("{:?}", self.input))
            .field("output", &format_args!("{:?}", self.output))
            .field("error", &self.error)
            .field("revert_reason", &format_args!("{:?}", self.revert_reason))
            .field("call_traces", &self.calls)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_call_deserialization() {
        let b = hex::decode("00000000002a000000000000003078303030303030303030303030303030303030303030303030303030303030303030303030303030302a00000000000000307830303030303030303030303030303030303030303030303030303030303030303030303038303031fa590600fa59060057ed03000f00000000000000307833386437656134633638303030000000000000000000000000000000000000010000000000000000000000002a000000000000003078303030303030303030303030303030303030303030303030303030303030303030303030383030312a00000000000000307830303030303030303030303030303030303030303030303030303030303030303030303038303062bae978fbda058bf7510f00000300000000000000307830040000000000000030e5ccbd000000000000000000000000000000000000").unwrap();
        let _: LegacyCall = bincode::deserialize(&b).unwrap();
    }

    #[test]
    fn call_deserialization() {
        let b = hex::decode("00000000002a000000000000003078303030303030303030303030303030303030303030303030303030303030303030303030303030302a00000000000000307830303030303030303030303030303030303030303030303030303030303030303030303038303031fa59060000000000fa5906000000000057ed0300000000000f00000000000000307833386437656134633638303030000000000000000000000000000000000000010000000000000000000000002a000000000000003078303030303030303030303030303030303030303030303030303030303030303030303030383030312a00000000000000307830303030303030303030303030303030303030303030303030303030303030303030303038303062bae978fb00000000da058bf700000000510f0000000000000300000000000000307830040000000000000030e5ccbd000000000000000000000000000000000000").unwrap();
        let _: Call = bincode::deserialize(&b).unwrap();
    }
}
