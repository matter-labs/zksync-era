use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use zksync_types::contract_verification_api::CompilationArtifacts;

pub(crate) use self::{
    solc::{Solc, SolcInput},
    zksolc::{ZkSolc, ZkSolcInput},
    zkvyper::{ZkVyper, ZkVyperInput},
};
use crate::error::ContractVerifierError;

mod solc;
mod zksolc;
mod zkvyper;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Source {
    /// The source code file content.
    pub content: String,
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
            .any(|err| err["severity"].as_str().unwrap() == "error")
        {
            let error_messages = errors
                .into_iter()
                .map(|err| err["formattedMessage"].clone())
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

    let Some(bytecode_str) = contract
        .pointer("/evm/bytecode/object")
        .context("missing bytecode in solc / zksolc output")?
        .as_str()
    else {
        return Err(ContractVerifierError::AbstractContract(contract_name));
    };
    let bytecode = hex::decode(bytecode_str).context("invalid bytecode")?;

    let deployed_bytecode = if get_deployed_bytecode {
        let bytecode_str = contract
            .pointer("/evm/deployedBytecode/object")
            .context("missing deployed bytecode in solc output")?
            .as_str()
            .ok_or(ContractVerifierError::AbstractContract(contract_name))?;
        Some(hex::decode(bytecode_str).context("invalid deployed bytecode")?)
    } else {
        None
    };

    let abi = contract["abi"].clone();
    if !abi.is_array() {
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
