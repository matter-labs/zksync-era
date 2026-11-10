//! Policy for compiler functionality exposed by the public contract-verification service.

use zksync_types::contract_verification::api::{
    CompilerVersions, SourceCodeData, VerificationIncomingRequest,
};

use crate::{
    compilers::{Solc, VyperInput, ZkSolc},
    error::ContractVerifierError,
    resolver::SupportedCompilerVersions,
};

// These switches intentionally live beside the version allowlist. A private build can restore a
// retained compiler capability by changing this policy without modifying compiler implementations.
const YUL_VERIFICATION_ENABLED: bool = false;
const VYPER_VERIFICATION_ENABLED: bool = false;
const SYSTEM_MODES_ENABLED: bool = false;

// Keep this list aligned with the verified binaries installed by the public Docker image.
const SUPPORTED_ZKSOLC_VERSIONS: &[&str] = &["1.5.15", "1.5.16", "1.5.17"];

/// Returns whether a zksolc version is exposed by the public verifier.
pub fn is_public_zksolc_version(version: &str) -> bool {
    let version = version.strip_prefix('v').unwrap_or(version);
    SUPPORTED_ZKSOLC_VERSIONS.contains(&version)
}

/// Returns whether Vyper toolchains are exposed by the public verifier.
pub fn is_public_vyper_enabled() -> bool {
    VYPER_VERIFICATION_ENABLED
}

/// Removes compiler capabilities that are intentionally unavailable through the public API.
pub(crate) fn retain_public_compiler_versions(versions: &mut SupportedCompilerVersions) {
    versions
        .zksolc
        .retain(|version| is_public_zksolc_version(version));
    if !is_public_vyper_enabled() {
        versions.vyper.clear();
        versions.zkvyper.clear();
    }
}

/// Validates a public verification request without resolving or executing a compiler. Building the
/// compiler input here also ensures that the request passes the same canonicalization used by the
/// worker before it can be queued.
pub fn validate_incoming_request(
    req: &VerificationIncomingRequest,
) -> Result<(), ContractVerifierError> {
    if req.source_code_data.compiler_type() != req.compiler_versions.compiler_type() {
        return Err(ContractVerifierError::UnsupportedVerificationInput(
            "source format and compiler toolchain do not match".to_owned(),
        ));
    }

    if let CompilerVersions::Solc {
        compiler_zksolc_version: Some(version),
        ..
    } = &req.compiler_versions
    {
        if !is_public_zksolc_version(version) {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "zksolc",
                version.clone(),
            ));
        }
    }

    if !SYSTEM_MODES_ENABLED
        && (req.is_system
            || req.force_evmla
            || standard_json_enables_privileged_mode(&req.source_code_data))
    {
        return Err(ContractVerifierError::UnsupportedVerificationInput(
            "system compilation modes are disabled".to_owned(),
        ));
    }

    if matches!(&req.source_code_data, SourceCodeData::YulSingleFile(_))
        && !YUL_VERIFICATION_ENABLED
    {
        return Err(ContractVerifierError::UnsupportedVerificationInput(
            "Yul verification is disabled".to_owned(),
        ));
    }
    if matches!(&req.source_code_data, SourceCodeData::VyperMultiFile(_))
        && !is_public_vyper_enabled()
    {
        return Err(ContractVerifierError::UnsupportedVerificationInput(
            "Vyper verification is disabled".to_owned(),
        ));
    }

    match &req.compiler_versions {
        CompilerVersions::Solc {
            compiler_zksolc_version: Some(version),
            ..
        } => ZkSolc::build_input(req.clone(), version).map(drop),
        CompilerVersions::Solc {
            compiler_zksolc_version: None,
            ..
        } => Solc::build_input(req.clone()).map(drop),
        CompilerVersions::Vyper { .. } => VyperInput::new(req.clone()).map(drop),
    }
}

fn standard_json_enables_privileged_mode(source: &SourceCodeData) -> bool {
    let SourceCodeData::StandardJsonInput(input) = source else {
        return false;
    };
    let Some(settings) = input.get("settings").and_then(serde_json::Value::as_object) else {
        return false;
    };
    [
        "enableEraVMExtensions",
        "isSystem",
        "forceEVMLA",
        "forceEvmla",
    ]
    .into_iter()
    .any(|name| settings.get(name).and_then(serde_json::Value::as_bool) == Some(true))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use zksync_types::contract_verification::api::{
        CompilerVersions, SourceCodeData, VerificationIncomingRequest,
    };

    use super::*;

    fn request(zksolc_version: &str) -> VerificationIncomingRequest {
        VerificationIncomingRequest {
            contract_address: Default::default(),
            source_code_data: SourceCodeData::SolSingleFile("contract Test {}".to_owned()),
            contract_name: "Test".to_owned(),
            compiler_versions: CompilerVersions::Solc {
                compiler_solc_version: "zkVM-0.8.30-1.0.2".to_owned(),
                compiler_zksolc_version: Some(zksolc_version.to_owned()),
            },
            optimization_used: true,
            optimizer_mode: None,
            constructor_arguments: Default::default(),
            is_system: false,
            force_evmla: false,
            evm_specific: Default::default(),
        }
    }

    #[test]
    fn exposes_only_selected_zksolc_versions() {
        for version in ["v1.5.15", "v1.5.16", "v1.5.17", "1.5.17"] {
            assert!(is_public_zksolc_version(version), "{version}");
            validate_incoming_request(&request(version)).unwrap();
        }
        for version in ["v1.5.14", "v1.4.1", "v1.5.18", "main"] {
            assert!(!is_public_zksolc_version(version), "{version}");
            assert!(matches!(
                validate_incoming_request(&request(version)),
                Err(ContractVerifierError::UnknownCompilerVersion("zksolc", _))
            ));
        }
    }

    #[test]
    fn filters_advertised_capabilities() {
        let mut versions = SupportedCompilerVersions {
            solc: HashSet::from(["0.8.30".to_owned()]),
            zksolc: HashSet::from([
                "v1.5.14".to_owned(),
                "v1.5.15".to_owned(),
                "v1.5.17".to_owned(),
            ]),
            vyper: HashSet::from(["0.3.10".to_owned()]),
            zkvyper: HashSet::from(["v1.5.4".to_owned()]),
        };

        retain_public_compiler_versions(&mut versions);

        assert_eq!(versions.solc, HashSet::from(["0.8.30".to_owned()]));
        assert_eq!(
            versions.zksolc,
            HashSet::from(["v1.5.15".to_owned(), "v1.5.17".to_owned()])
        );
        assert!(versions.vyper.is_empty());
        assert!(versions.zkvyper.is_empty());
    }

    #[test]
    fn rejects_privileged_standard_json_modes() {
        for setting in [
            "enableEraVMExtensions",
            "isSystem",
            "forceEVMLA",
            "forceEvmla",
        ] {
            let mut req = request("v1.5.17");
            req.source_code_data = SourceCodeData::StandardJsonInput(
                serde_json::json!({
                    "language": "Solidity",
                    "sources": { "Test.sol": { "content": "contract Test {}" } },
                    "settings": { (setting): true },
                })
                .as_object()
                .unwrap()
                .clone(),
            );

            assert!(matches!(
                validate_incoming_request(&req),
                Err(ContractVerifierError::UnsupportedVerificationInput(_))
            ));
        }
    }

    #[test]
    fn rejects_dormant_source_formats() {
        let mut yul = request("v1.5.17");
        yul.source_code_data = SourceCodeData::YulSingleFile("object \"Test\" {}".to_owned());
        assert!(matches!(
            validate_incoming_request(&yul),
            Err(ContractVerifierError::UnsupportedVerificationInput(_))
        ));

        let mut vyper = request("v1.5.17");
        vyper.source_code_data =
            SourceCodeData::VyperMultiFile([("Test.vy".to_owned(), String::new())].into());
        vyper.compiler_versions = CompilerVersions::Vyper {
            compiler_vyper_version: "0.3.10".to_owned(),
            compiler_zkvyper_version: None,
        };
        assert!(matches!(
            validate_incoming_request(&vyper),
            Err(ContractVerifierError::UnsupportedVerificationInput(_))
        ));
    }
}
