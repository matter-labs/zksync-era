use std::collections::HashMap;

use anyhow::Context as _;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zksync_types::{
    contract_verification::api::{CompilationArtifacts, ImmutableReference},
    H256,
};

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

const MAX_SOURCE_COUNT: usize = 512;
const MAX_SOURCE_PATH_BYTES: usize = 256;
const MAX_SOURCE_BYTES: usize = 512 * 1024;
const MAX_TOTAL_SOURCE_BYTES: usize = 3 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompilerFlavor {
    Solc,
    ZkSolc,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StandardJson {
    pub language: String,
    pub sources: HashMap<String, Source>,
    #[serde(default)]
    pub settings: Settings,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Settings {
    /// Accepted for API compatibility, but always replaced with a verifier-owned selection.
    pub output_selection: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimizer: Option<Optimizer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub libraries: Option<HashMap<String, HashMap<String, String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evm_version: Option<String>,
    #[serde(rename = "viaIR", skip_serializing_if = "Option::is_none")]
    pub via_ir: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<DebugSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codegen: Option<String>,

    // These modes are retained for direct / private compiler use. Public requests reject enabled
    // values before compiler input is built.
    #[serde(
        rename = "enableEraVMExtensions",
        skip_serializing_if = "is_none_or_false"
    )]
    pub(crate) enable_eravm_extensions: Option<bool>,
    #[serde(rename = "forceEVMLA", skip_serializing_if = "is_none_or_false")]
    pub(crate) force_evmla: Option<bool>,
    // Older requests use these spellings. They are normalized to the canonical fields above.
    #[serde(rename = "isSystem", skip_serializing)]
    legacy_is_system: Option<bool>,
    #[serde(rename = "forceEvmla", skip_serializing)]
    legacy_force_evmla: Option<bool>,

    // Known legacy wrapper fields. Their values are accepted for wire compatibility but never
    // forwarded to a compiler. Keeping them here avoids breaking ordinary requests produced by
    // old tooling while forbidding the underlying operations.
    #[serde(skip_serializing)]
    detect_missing_libraries: Option<bool>,
    #[serde(skip_serializing)]
    are_libraries_missing: Option<bool>,
    #[serde(skip_serializing)]
    enabled: Option<bool>,
    #[serde(skip_serializing)]
    runs: Option<u32>,
    /// Remappings have historically exposed filesystem access, so they are never forwarded.
    #[serde(skip_serializing)]
    remappings: Option<Value>,
}

fn is_none_or_false(value: &Option<bool>) -> bool {
    !value.unwrap_or(false)
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Optimizer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Solidity uses this as a code-size / runtime-cost weighting, not as an optimizer iteration
    /// count. The `u32` wire type is therefore the useful bound; a smaller ceiling only prevents
    /// reproducing otherwise ordinary builds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    // Known zksolc / wrapper extensions. Their evolving semantics are intentionally not
    // interpreted by the public verifier; values are accepted for compatibility and erased.
    #[serde(rename = "disable_system_request_memoization", skip_serializing)]
    disable_system_request_memoization: Option<bool>,
    #[serde(rename = "fallback_to_optimizing_for_size", skip_serializing)]
    fallback_to_optimizing_for_size: Option<bool>,
    #[serde(rename = "fallbackToOptimizingForSize", skip_serializing)]
    fallback_to_optimizing_for_size_camel: Option<bool>,
    #[serde(rename = "size_fallback", skip_serializing)]
    size_fallback: Option<bool>,
    #[serde(skip_serializing)]
    codegen: Option<String>,
    #[serde(rename = "suppressedErrors", skip_serializing)]
    suppressed_errors: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bytecode_hash: Option<String>,
    /// zksolc 1.5.x calls Solidity's `bytecodeHash` setting `hashType`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hash_type: Option<String>,
    #[serde(rename = "appendCBOR", skip_serializing_if = "Option::is_none")]
    pub(crate) append_cbor: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) use_literal_content: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DebugSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    revert_strings: Option<RevertStrings>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum RevertStrings {
    Default,
    Strip,
    Debug,
    VerboseDebug,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Source {
    /// The source code file content.
    pub content: String,
}

/// Validates that all source path keys are relative and contain no traversal components.
/// Absolute paths (`/foo`) and parent-directory references (`../foo`) allow the compiler
/// to read arbitrary files from the container filesystem, so both are rejected here.
pub(crate) fn validate_source_paths(
    sources: &HashMap<String, Source>,
) -> Result<(), ContractVerifierError> {
    for path in sources.keys() {
        if path.is_empty()
            || path.len() > MAX_SOURCE_PATH_BYTES
            || path.starts_with('/')
            || path.starts_with("file://")
            || path.contains(['\\', '\0', ':'])
            || path
                .split('/')
                .any(|component| component.is_empty() || component == "." || component == "..")
            || !path.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'_' | b'-' | b'.' | b'/' | b'@' | b'+' | b'$')
            })
        {
            return Err(ContractVerifierError::InvalidSourcePath(path.clone()));
        }
    }
    Ok(())
}

pub(crate) fn parse_standard_json_input(
    map: serde_json::Map<String, Value>,
    flavor: CompilerFlavor,
) -> Result<StandardJson, ContractVerifierError> {
    let mut input: StandardJson = serde_json::from_value(Value::Object(map)).map_err(|err| {
        tracing::debug!(%err, "rejected non-canonical standard JSON compiler input");
        ContractVerifierError::FailedToDeserializeInput
    })?;
    input.settings.normalize(flavor)?;
    input.validate(flavor)?;
    Ok(input)
}

impl StandardJson {
    pub(crate) fn validate(&self, flavor: CompilerFlavor) -> Result<(), ContractVerifierError> {
        if self.language != "Solidity" {
            return Err(ContractVerifierError::UnsupportedVerificationInput(
                "only Solidity sources are accepted".to_owned(),
            ));
        }
        self.validate_sources(true)?;
        self.settings.validate(flavor)
    }

    pub(crate) fn validate_yul(&self, flavor: CompilerFlavor) -> Result<(), ContractVerifierError> {
        if self.language != "Yul" {
            return Err(ContractVerifierError::UnsupportedVerificationInput(
                "expected Yul sources".to_owned(),
            ));
        }
        self.validate_sources(false)?;
        self.settings.validate(flavor)
    }

    fn validate_sources(&self, check_import_roots: bool) -> Result<(), ContractVerifierError> {
        if self.sources.is_empty() || self.sources.len() > MAX_SOURCE_COUNT {
            return Err(ContractVerifierError::UnsupportedVerificationInput(
                "source count is outside the allowed range".to_owned(),
            ));
        }
        validate_source_paths(&self.sources)?;

        let mut total_source_bytes = 0usize;
        for source in self.sources.values() {
            let source_bytes = source.content.len();
            if source_bytes > MAX_SOURCE_BYTES {
                return Err(ContractVerifierError::UnsupportedVerificationInput(
                    "a source file exceeds the allowed size".to_owned(),
                ));
            }
            total_source_bytes = total_source_bytes.saturating_add(source_bytes);
            if total_source_bytes > MAX_TOTAL_SOURCE_BYTES {
                return Err(ContractVerifierError::UnsupportedVerificationInput(
                    "total source size exceeds the allowed limit".to_owned(),
                ));
            }
            if check_import_roots && has_unsupported_import_roots(&source.content) {
                return Err(ContractVerifierError::InvalidSourcePath(
                    "import with absolute path".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

impl Settings {
    fn normalize(&mut self, flavor: CompilerFlavor) -> Result<(), ContractVerifierError> {
        self.enable_eravm_extensions =
            merge_legacy_bool(self.enable_eravm_extensions, self.legacy_is_system.take())?;
        self.force_evmla = merge_legacy_bool(self.force_evmla, self.legacy_force_evmla.take())?;

        if let Some(metadata) = &mut self.metadata {
            if metadata.bytecode_hash.is_some() && metadata.hash_type.is_some() {
                return Err(ContractVerifierError::UnsupportedVerificationInput(
                    "specify only one metadata hash setting".to_owned(),
                ));
            }
            match flavor {
                CompilerFlavor::Solc => {
                    metadata.bytecode_hash =
                        metadata.hash_type.take().or(metadata.bytecode_hash.take());
                }
                CompilerFlavor::ZkSolc => {
                    metadata.hash_type =
                        metadata.bytecode_hash.take().or(metadata.hash_type.take());
                }
            }
        }
        Ok(())
    }

    fn validate(&self, flavor: CompilerFlavor) -> Result<(), ContractVerifierError> {
        let normalized_flags = [
            ("detectMissingLibraries", self.detect_missing_libraries),
            ("areLibrariesMissing", self.are_libraries_missing),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.is_some().then_some(name))
        .collect::<Vec<_>>();
        // Remappings are never forwarded to a compiler. An empty list is inert and stays accepted.
        let has_remappings = match &self.remappings {
            Some(Value::Array(entries)) => !entries.is_empty(),
            Some(Value::Null) | None => false,
            Some(_) => true,
        };
        if has_remappings {
            return Err(ContractVerifierError::UnsupportedVerificationInput(
                "`settings.remappings` is not supported; imports must match `sources` keys exactly"
                    .to_owned(),
            ));
        }
        if self.runs.is_some() || !normalized_flags.is_empty() {
            tracing::debug!(
                ?normalized_flags,
                has_legacy_optimizer_runs = self.runs.is_some(),
                "erasing unsupported legacy compiler settings"
            );
        }
        if flavor == CompilerFlavor::Solc
            && (self.enable_eravm_extensions == Some(true) || self.force_evmla == Some(true))
        {
            return Err(ContractVerifierError::UnsupportedVerificationInput(
                "system compilation modes require zksolc".to_owned(),
            ));
        }
        if let (Some(enabled), Some(optimizer_enabled)) = (
            self.enabled,
            self.optimizer
                .as_ref()
                .and_then(|optimizer| optimizer.enabled),
        ) {
            if enabled != optimizer_enabled {
                return Err(ContractVerifierError::UnsupportedVerificationInput(
                    "conflicting optimizer settings".to_owned(),
                ));
            }
        }
        if let Some(optimizer) = &self.optimizer {
            optimizer.validate(flavor)?;
        }
        if let Some(evm_version) = &self.evm_version {
            const ALLOWED_EVM_VERSIONS: &[&str] = &[
                "homestead",
                "tangerineWhistle",
                "spuriousDragon",
                "byzantium",
                "constantinople",
                "petersburg",
                "istanbul",
                "berlin",
                "london",
                "paris",
                "shanghai",
                "cancun",
                "prague",
                "osaka",
            ];
            if !ALLOWED_EVM_VERSIONS.contains(&evm_version.as_str()) {
                return Err(ContractVerifierError::UnsupportedVerificationInput(
                    "unsupported EVM version".to_owned(),
                ));
            }
        }
        if let Some(codegen) = &self.codegen {
            if flavor != CompilerFlavor::ZkSolc || codegen != "yul" {
                return Err(ContractVerifierError::UnsupportedVerificationInput(
                    "unsupported code generation mode".to_owned(),
                ));
            }
        }
        if let Some(metadata) = &self.metadata {
            metadata.validate()?;
        }
        if let Some(libraries) = &self.libraries {
            for (source_path, source_libraries) in libraries {
                validate_source_paths(&HashMap::from([(
                    source_path.clone(),
                    Source {
                        content: String::new(),
                    },
                )]))?;
                for (contract_name, address) in source_libraries {
                    if !is_solidity_identifier(contract_name) || !is_address(address) {
                        return Err(ContractVerifierError::UnsupportedVerificationInput(
                            "invalid library name or address".to_owned(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn system_mode_enabled(&self) -> bool {
        self.enable_eravm_extensions == Some(true)
    }

    pub(crate) fn force_evmla_enabled(&self) -> bool {
        self.force_evmla == Some(true)
    }
}

fn merge_legacy_bool(
    canonical: Option<bool>,
    legacy: Option<bool>,
) -> Result<Option<bool>, ContractVerifierError> {
    match (canonical, legacy) {
        (Some(canonical), Some(legacy)) if canonical != legacy => {
            Err(ContractVerifierError::UnsupportedVerificationInput(
                "conflicting system compilation settings".to_owned(),
            ))
        }
        (canonical, legacy) => Ok(canonical.or(legacy)),
    }
}

impl Optimizer {
    fn validate(&self, flavor: CompilerFlavor) -> Result<(), ContractVerifierError> {
        if let Some(mode) = &self.mode {
            if flavor != CompilerFlavor::ZkSolc || !matches!(mode.as_str(), "3" | "z") {
                return Err(ContractVerifierError::UnsupportedVerificationInput(
                    "unsupported optimizer mode".to_owned(),
                ));
            }
        }
        if self.disable_system_request_memoization.is_some()
            || self.fallback_to_optimizing_for_size.is_some()
            || self.fallback_to_optimizing_for_size_camel.is_some()
            || self.size_fallback.is_some()
            || self.codegen.is_some()
            || self.suppressed_errors.is_some()
        {
            tracing::debug!("erasing unsupported legacy optimizer settings");
        }
        Ok(())
    }
}

impl Metadata {
    fn validate(&self) -> Result<(), ContractVerifierError> {
        if self
            .bytecode_hash
            .as_ref()
            .or(self.hash_type.as_ref())
            .map(String::as_str)
            .is_some_and(|hash| !matches!(hash, "none" | "ipfs" | "bzzr1" | "keccak256"))
        {
            return Err(ContractVerifierError::UnsupportedVerificationInput(
                "unsupported metadata hash mode".to_owned(),
            ));
        }
        Ok(())
    }
}

fn is_solidity_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || matches!(first, b'_' | b'$'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
        && value.len() <= 128
}

fn is_address(value: &str) -> bool {
    let value = value.strip_prefix("0x").unwrap_or(value);
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn validate_contract_target(
    file_name: &str,
    contract_name: &str,
) -> Result<(), ContractVerifierError> {
    validate_source_paths(&HashMap::from([(
        file_name.to_owned(),
        Source {
            content: String::new(),
        },
    )]))?;
    if !is_solidity_identifier(contract_name) {
        return Err(ContractVerifierError::UnsupportedVerificationInput(
            "invalid contract name".to_owned(),
        ));
    }
    Ok(())
}

/// Returns `true` if `source` contains an `import` directive whose path is absolute (`/…`)
/// or uses a `file://` URL. These import roots are outside the submitted source map.
///
/// Relative imports containing `../` are allowed: they are standard Solidity practice and
/// are handled by source-path validation plus the empty compiler search directory.
pub(crate) fn has_unsupported_import_roots(source: &str) -> bool {
    // Covers all Solidity import forms:
    //   import "/path";
    //   import {X} from "/path";
    //   import * as X from "/path";
    //   import "file:///path";
    let re = Regex::new(r#"\bimport\b[^;]*?["'](?:/|file://)"#).unwrap();
    re.is_match(source)
}

/// Strips pipe-prefixed source-snippet lines from a compiler `formattedMessage`.
///
/// The `formattedMessage` format looks like:
/// ```text
/// ParserError: Expected ';' but got end of source
///  --> Source.sol:1:5:
///   |
/// 1 | INVALID_SOURCE_LINE
///   |     ^
/// ```
/// Numbered source lines and caret lines are omitted to keep diagnostics concise. The
/// ` --> path:line:col` header is preserved for location context.
fn is_source_context_line(line: &str) -> bool {
    let line = line.trim_start();
    if line.starts_with('|') {
        return true;
    }

    let digit_count = line
        .bytes()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    digit_count > 0 && line[digit_count..].trim_start().starts_with('|')
}

fn strip_source_snippets(msg: &str) -> String {
    msg.lines()
        .filter(|line| !is_source_context_line(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strips source-context lines from raw compiler stderr before returning diagnostics from the
/// non-JSON (exit-code != 0) error path.
pub(crate) fn sanitize_compiler_stderr(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| !line.contains(" --> ") && !is_source_context_line(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        has_unsupported_import_roots, parse_standard_json_input, sanitize_compiler_stderr,
        strip_source_snippets, validate_source_paths, CompilerFlavor, Source,
    };

    #[test]
    fn rejects_noncanonical_source_paths() {
        for path in [
            "../etc/passwd",
            "src/../../etc/passwd",
            "/etc/passwd",
            "file:///etc/passwd",
            "./Counter.sol",
            "src//Counter.sol",
            "src\\Counter.sol",
            "C:/Counter.sol",
            "src/Counter.sol\0suffix",
        ] {
            let sources = std::collections::HashMap::from([(
                path.to_owned(),
                Source {
                    content: String::new(),
                },
            )]);
            assert!(
                validate_source_paths(&sources).is_err(),
                "accepted {path:?}"
            );
        }

        let sources = std::collections::HashMap::from([(
            "@openzeppelin/contracts/token/ERC20/ERC20.sol".to_owned(),
            Source {
                content: String::new(),
            },
        )]);
        validate_source_paths(&sources).unwrap();
    }

    #[test]
    fn rejects_arbitrary_compiler_options() {
        for settings in [
            serde_json::json!({ "LLVMOptions": ["--exec-on-ir-change=/bin/sh"] }),
            serde_json::json!({ "llvmOptions": ["--exec-on-ir-change=/bin/sh"] }),
            serde_json::json!({ "viaIr": true }),
            serde_json::json!({ "optimizer": { "details": {} } }),
        ] {
            let input = serde_json::json!({
                "language": "Solidity",
                "sources": { "Counter.sol": { "content": "contract Counter {}" } },
                "settings": settings,
            });
            let err = parse_standard_json_input(
                input.as_object().unwrap().clone(),
                CompilerFlavor::ZkSolc,
            )
            .unwrap_err();
            assert!(
                matches!(
                    err,
                    crate::error::ContractVerifierError::FailedToDeserializeInput
                        | crate::error::ContractVerifierError::UnsupportedVerificationInput(_)
                ),
                "unexpected error: {err:?}"
            );
        }
    }

    #[test]
    fn erases_unsafe_options_but_retains_private_system_modes() {
        let input = serde_json::json!({
            "language": "Solidity",
            "sources": { "Counter.sol": { "content": "contract Counter {}" } },
            "settings": {
                "isSystem": true,
                "forceEvmla": true,
                "optimizer": {
                    "enabled": true,
                    "mode": "3",
                    "disable_system_request_memoization": true
                }
            },
        });
        let input =
            parse_standard_json_input(input.as_object().unwrap().clone(), CompilerFlavor::ZkSolc)
                .unwrap();
        let serialized = serde_json::to_string(&input).unwrap();
        assert!(
            !serialized.contains("disable_system_request_memoization"),
            "erased optimizer flag reached compiler input: {serialized}"
        );
        let serialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(
            serialized["settings"]["enableEraVMExtensions"],
            serde_json::json!(true)
        );
        assert_eq!(
            serialized["settings"]["forceEVMLA"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn rejects_populated_remappings() {
        let with_remappings = serde_json::json!({
            "language": "Solidity",
            "sources": { "src/A.sol": { "content": "contract A {}" } },
            "settings": { "remappings": ["solady/=lib/solady/"] },
        });
        let err = parse_standard_json_input(
            with_remappings.as_object().unwrap().clone(),
            CompilerFlavor::ZkSolc,
        )
        .unwrap_err();
        let crate::error::ContractVerifierError::UnsupportedVerificationInput(message) = &err
        else {
            panic!("unexpected error: {err:?}");
        };
        assert!(message.contains("remappings"), "{message}");
        assert!(message.contains("sources"), "{message}");

        // An empty list changes nothing, so it must not be rejected.
        let empty = serde_json::json!({
            "language": "Solidity",
            "sources": { "src/A.sol": { "content": "contract A {}" } },
            "settings": { "remappings": [] },
        });
        parse_standard_json_input(empty.as_object().unwrap().clone(), CompilerFlavor::ZkSolc)
            .expect("an empty remappings list is inert");
    }

    #[test]
    fn normalizes_metadata_hash_field_for_each_compiler() {
        let input = serde_json::json!({
            "language": "Solidity",
            "sources": { "Counter.sol": { "content": "contract Counter {}" } },
            "settings": { "metadata": { "bytecodeHash": "ipfs", "appendCBOR": true } },
        });
        let zksolc_input =
            parse_standard_json_input(input.as_object().unwrap().clone(), CompilerFlavor::ZkSolc)
                .unwrap();
        let serialized = serde_json::to_value(zksolc_input).unwrap();
        assert_eq!(serialized["settings"]["metadata"]["hashType"], "ipfs");
        assert_eq!(serialized["settings"]["metadata"]["appendCBOR"], true);
        assert!(serialized["settings"]["metadata"]
            .get("bytecodeHash")
            .is_none());

        let input = serde_json::json!({
            "language": "Solidity",
            "sources": { "Counter.sol": { "content": "contract Counter {}" } },
            "settings": { "metadata": { "hashType": "none" } },
        });
        let solc_input =
            parse_standard_json_input(input.as_object().unwrap().clone(), CompilerFlavor::Solc)
                .unwrap();
        let serialized = serde_json::to_value(solc_input).unwrap();
        assert_eq!(serialized["settings"]["metadata"]["bytecodeHash"], "none");
        assert!(serialized["settings"]["metadata"].get("hashType").is_none());
    }

    #[test]
    fn accepts_canonical_via_ir_setting() {
        let input = serde_json::json!({
            "language": "Solidity",
            "sources": { "Counter.sol": { "content": "contract Counter {}" } },
            "settings": { "viaIR": true },
        });
        let input =
            parse_standard_json_input(input.as_object().unwrap().clone(), CompilerFlavor::ZkSolc)
                .unwrap();

        let serialized = serde_json::to_value(input).unwrap();
        assert_eq!(serialized["settings"]["viaIR"], true);
        assert!(serialized["settings"].get("viaIr").is_none());
    }

    #[test]
    fn accepts_standard_debug_revert_string_modes() {
        for revert_strings in ["default", "strip", "debug", "verboseDebug"] {
            let input = serde_json::json!({
                "language": "Solidity",
                "sources": { "Counter.sol": { "content": "contract Counter {}" } },
                "settings": { "debug": { "revertStrings": revert_strings } },
            });
            let input =
                parse_standard_json_input(input.as_object().unwrap().clone(), CompilerFlavor::Solc)
                    .unwrap();

            let serialized = serde_json::to_value(input).unwrap();
            assert_eq!(
                serialized["settings"]["debug"]["revertStrings"],
                revert_strings
            );
        }

        for debug in [
            serde_json::json!({ "revertStrings": "arbitrary" }),
            serde_json::json!({ "unexpected": true }),
        ] {
            let input = serde_json::json!({
                "language": "Solidity",
                "sources": { "Counter.sol": { "content": "contract Counter {}" } },
                "settings": { "debug": debug },
            });
            assert!(parse_standard_json_input(
                input.as_object().unwrap().clone(),
                CompilerFlavor::Solc,
            )
            .is_err());
        }
    }

    #[test]
    fn accepts_full_u32_optimizer_runs_range() {
        let input = serde_json::json!({
            "language": "Solidity",
            "sources": { "Counter.sol": { "content": "contract Counter {}" } },
            "settings": { "optimizer": { "enabled": true, "runs": u32::MAX } },
        });

        let input =
            parse_standard_json_input(input.as_object().unwrap().clone(), CompilerFlavor::Solc)
                .unwrap();
        let serialized = serde_json::to_value(input).unwrap();
        assert_eq!(serialized["settings"]["optimizer"]["runs"], u32::MAX);
    }

    #[test]
    fn allows_relative_parent_imports() {
        let source = r#"
            import {ContextUpgradeable} from "../utils/ContextUpgradeable.sol";
            import {Hashes} from "./Hashes.sol";
        "#;

        assert!(
            !has_unsupported_import_roots(source),
            "relative imports within the submitted source tree must be allowed"
        );
    }

    #[test]
    fn rejects_absolute_imports() {
        assert!(has_unsupported_import_roots(
            r#"import "/absolute/path/Source.sol";"#
        ));
        assert!(has_unsupported_import_roots(
            r#"import "file:///absolute/path/Source.sol";"#
        ));
    }

    #[test]
    fn normalizes_formatted_message_source_context() {
        let message = "ParserError: invalid source\n --> Source.sol:12:1:\n   |\n12 | INVALID_SOURCE_LINE\n   | ^^^^^^^^^^^^^^^^^^^\n";

        let sanitized = strip_source_snippets(message);

        assert!(sanitized.contains("ParserError: invalid source"));
        assert!(sanitized.contains(" --> Source.sol:12:1:"));
        assert!(!sanitized.contains("INVALID_SOURCE_LINE"));
        assert!(!sanitized.contains("12 |"));
    }

    #[test]
    fn normalizes_compiler_stderr_source_context() {
        let stderr = "ParserError: invalid source\n --> Source.sol:1:1:\n  |\n1 | INVALID_SOURCE_LINE\n  | ^^^^^^^^^^^^^^^^^^^\n";

        let sanitized = sanitize_compiler_stderr(stderr);

        assert!(sanitized.contains("ParserError: invalid source"));
        assert!(!sanitized.contains("Source.sol"));
        assert!(!sanitized.contains("INVALID_SOURCE_LINE"));
        assert!(!sanitized.contains("1 |"));
    }
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

/// Parses `/evm/deployedBytecode/immutableReferences`
/// If the path doesn't exist or isn't an object, returns `None`.
fn parse_immutable_refs(
    refs_val: Option<&Value>,
) -> Option<HashMap<String, Vec<ImmutableReference>>> {
    let obj = refs_val?.as_object()?;

    let mut map = HashMap::new();
    for (placeholder_key, spans_val) in obj {
        if let Some(spans_arr) = spans_val.as_array() {
            let mut spans_vec = Vec::new();
            for item in spans_arr {
                let start = item
                    .get("start")
                    .and_then(|v| v.as_u64())
                    .unwrap_or_default() as usize;
                let length = item
                    .get("length")
                    .and_then(|v| v.as_u64())
                    .unwrap_or_default() as usize;
                spans_vec.push(ImmutableReference { start, length });
            }
            if !spans_vec.is_empty() {
                map.insert(placeholder_key.clone(), spans_vec);
            }
        }
    }

    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

/// Collects the 32-byte factory dependency bytecode hashes reported in the compiler output.
/// The verifier uses these values to locate dependency-hash words during bytecode comparison
/// (see [`CompilationArtifacts::patch_immutable_bytecodes`]); the link offsets are not needed.
fn parse_factory_dependency_hashes(contract: &Value) -> Vec<H256> {
    let Some(deps) = contract
        .get("factoryDependencies")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };

    deps.keys()
        .filter_map(|hash| {
            let hash = hex::decode(hash.strip_prefix("0x").unwrap_or(hash)).ok()?;
            (hash.len() == 32).then(|| H256::from_slice(&hash))
        })
        .collect()
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
        if errors.iter().any(|err| {
            err["severity"].as_str() == Some("error")
                && !err["message"]
                    .as_str()
                    .map(is_suppressable_error)
                    .unwrap_or(false)
        }) {
            let error_messages = errors
                .into_iter()
                .filter_map(|err| {
                    let raw = err
                        .get("formattedMessage")
                        .or_else(|| err.get("message"))?
                        .as_str()?;
                    Some(serde_json::Value::String(strip_source_snippets(raw)))
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
        return Err(ContractVerifierError::MissingCompilerOutput {
            contract_name,
            field_path: "/evm/bytecode/object",
        });
    };
    let bytecode_str = bytecode_str
        .as_str()
        .context("unexpected `/evm/bytecode/object` value")?;
    // Strip an optional `0x` prefix (output by `vyper`, but not by `solc` / `zksolc`)
    let bytecode_str = bytecode_str.strip_prefix("0x").unwrap_or(bytecode_str);
    let bytecode = hex::decode(bytecode_str).context("invalid bytecode")?;

    let deployed_bytecode = if get_deployed_bytecode {
        let Some(bytecode_str) = contract.pointer("/evm/deployedBytecode/object") else {
            return Err(ContractVerifierError::MissingCompilerOutput {
                contract_name,
                field_path: "/evm/deployedBytecode/object",
            });
        };
        let bytecode_str = bytecode_str
            .as_str()
            .context("unexpected `/evm/deployedBytecode/object` value")?;
        let bytecode_str = bytecode_str.strip_prefix("0x").unwrap_or(bytecode_str);
        Some(hex::decode(bytecode_str).context("invalid deployed bytecode")?)
    } else {
        None
    };

    // Need to extract immutable references if any are present
    let immutable_refs =
        parse_immutable_refs(contract.pointer("/evm/deployedBytecode/immutableReferences"))
            .unwrap_or_default();
    let factory_dependency_hashes = parse_factory_dependency_hashes(contract);

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
        immutable_refs,
        factory_dependency_hashes,
    })
}

fn is_suppressable_error(message: &str) -> bool {
    // `zksolc` can produce warnings with `Error` severity that can be suppressed.
    // We want to filter out such messages.
    // All of them mention `suppressedErrors` in the message, which is a custom
    // `zksolc` configuration, so we use it as a marker.
    message.contains("suppressedErrors")
}

#[cfg(test)]
mod parser_tests {
    use super::parse_standard_json_output;
    use crate::error::ContractVerifierError;

    #[test]
    fn reports_missing_creation_bytecode_path() {
        let output = serde_json::json!({
            "contracts": {
                "Counter.sol": {
                    "Counter": {
                        "abi": []
                    }
                }
            }
        });

        let err = parse_standard_json_output(
            &output,
            "Counter".to_owned(),
            "Counter.sol".to_owned(),
            false,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            ContractVerifierError::MissingCompilerOutput {
                contract_name,
                field_path: "/evm/bytecode/object",
            } if contract_name == "Counter"
        ));
    }

    #[test]
    fn reports_missing_deployed_bytecode_path() {
        let output = serde_json::json!({
            "contracts": {
                "Counter.sol": {
                    "Counter": {
                        "abi": [],
                        "evm": {
                            "bytecode": {
                                "object": "00"
                            }
                        }
                    }
                }
            }
        });

        let err = parse_standard_json_output(
            &output,
            "Counter".to_owned(),
            "Counter.sol".to_owned(),
            true,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            ContractVerifierError::MissingCompilerOutput {
                contract_name,
                field_path: "/evm/deployedBytecode/object",
            } if contract_name == "Counter"
        ));
    }

    #[test]
    fn parses_factory_dependency_hashes() {
        let dep_a = "010002f3aa6cac6815f2300b1a4ed078983900fa5a0268f6575db307b09ae610";
        let dep_b = "010004a1bb6cac6815f2300b1a4ed078983900fa5a0268f6575db307b09ae611";
        // Hashes come from the `factoryDependencies` keys; the bytecode content is not scanned.
        let output = serde_json::json!({
            "contracts": {
                "Counter.sol": {
                    "Counter": {
                        "abi": [],
                        "evm": {
                            "bytecode": {
                                "object": "00",
                            }
                        },
                        "factoryDependencies": {
                            dep_a: "A.sol:A",
                            dep_b: "B.sol:B",
                        }
                    }
                }
            }
        });

        let artifacts = parse_standard_json_output(
            &output,
            "Counter".to_owned(),
            "Counter.sol".to_owned(),
            false,
        )
        .unwrap();

        let mut hashes = artifacts.factory_dependency_hashes;
        hashes.sort();
        let mut expected = vec![
            zksync_types::H256::from_slice(&hex::decode(dep_a).unwrap()),
            zksync_types::H256::from_slice(&hex::decode(dep_b).unwrap()),
        ];
        expected.sort();
        assert_eq!(hashes, expected);
    }
}
