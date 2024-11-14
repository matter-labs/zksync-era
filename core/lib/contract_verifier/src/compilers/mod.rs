use std::collections::HashMap;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use zksync_types::contract_verification_api::CompilationArtifacts;

pub(crate) use self::{
    solc::{Solc, SolcInput},
    vyper::{Vyper, VyperInput},
    zksolc::{ZkSolc, ZkSolcInput},
    zkvyper::ZkVyper,
};
use crate::error::ContractVerifierError;

mod solc;
mod vyper;
mod zksolc;
mod zkvyper;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StandardJson {
    pub language: String,
    pub sources: HashMap<String, Source>,
    #[serde(default)]
    settings: Settings,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    /// The output selection filters.
    output_selection: Option<serde_json::Value>,
    /// Other settings (only filled when parsing `StandardJson` input from the request).
    #[serde(flatten)]
    other: serde_json::Value,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_selection: None,
            other: serde_json::json!({}),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Source {
    /// The source code file content.
    pub content: String,
}

/// Users may provide either just contract name or source file name and contract name joined with ":".
fn process_contract_name(original_name: &str, extension: &str) -> (String, String) {
    if let Some((file_name, contract_name)) = original_name.rsplit_once(':') {
        (file_name.to_owned(), contract_name.to_owned())
    } else {
        (
            format!("{original_name}.{extension}"),
            original_name.to_owned(),
        )
    }
}

/// Parsing logic shared between `solc` and `zksolc`.
fn parse_standard_json_output(
    output: &serde_json::Value,
    contract_name: String,
    file_name: String,
    get_deployed_bytecode: bool,
) -> Result<CompilationArtifacts, ContractVerifierError> {
    if let Some(errors) = output.get("errors") {
        let errors = errors.as_array().unwrap().clone();
        if errors
            .iter()
            .any(|err| err["severity"].as_str() == Some("error"))
        {
            let error_messages = errors
                .into_iter()
                .filter_map(|err| {
                    // `formattedMessage` is an optional field
                    err.get("formattedMessage")
                        .or_else(|| err.get("message"))
                        .cloned()
                })
                .collect();
            return Err(ContractVerifierError::CompilationError(
                serde_json::Value::Array(error_messages),
            ));
        }
    }

    let contracts = output["contracts"]
        .get(&file_name)
        .ok_or(ContractVerifierError::MissingSource(file_name))?;
    let Some(contract) = contracts.get(&contract_name) else {
        return Err(ContractVerifierError::MissingContract(contract_name));
    };

    let Some(bytecode_str) = contract.pointer("/evm/bytecode/object") else {
        return Err(ContractVerifierError::AbstractContract(contract_name));
    };
    let bytecode_str = bytecode_str
        .as_str()
        .context("unexpected `/evm/bytecode/object` value")?;
    // Strip an optional `0x` prefix (output by `vyper`, but not by `solc` / `zksolc`)
    let bytecode_str = bytecode_str.strip_prefix("0x").unwrap_or(bytecode_str);
    let bytecode = hex::decode(bytecode_str).context("invalid bytecode")?;

    let deployed_bytecode = if get_deployed_bytecode {
        let Some(bytecode_str) = contract.pointer("/evm/deployedBytecode/object") else {
            return Err(ContractVerifierError::AbstractContract(contract_name));
        };
        let bytecode_str = bytecode_str
            .as_str()
            .context("unexpected `/evm/deployedBytecode/object` value")?;
        let bytecode_str = bytecode_str.strip_prefix("0x").unwrap_or(bytecode_str);
        Some(hex::decode(bytecode_str).context("invalid deployed bytecode")?)
    } else {
        None
    };

    let mut abi = contract["abi"].clone();
    if abi.is_null() {
        // ABI is undefined for Yul contracts when compiled with standalone `solc`. For uniformity with `zksolc`,
        // replace it with an empty array.
        abi = serde_json::json!([]);
    } else if !abi.is_array() {
        let err = anyhow::anyhow!(
            "unexpected value for ABI: {}",
            serde_json::to_string_pretty(&abi).unwrap()
        );
        return Err(err.into());
    }

    Ok(CompilationArtifacts {
        bytecode,
        deployed_bytecode,
        abi,
    })
}
