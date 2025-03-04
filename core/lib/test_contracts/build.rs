use std::{
    collections::{HashMap, HashSet},
    env,
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use foundry_compilers::{
    artifacts::{
        zksolc::output_selection::{FileOutputSelection, OutputSelection, OutputSelectionFlag},
        Remapping,
    },
    solc,
    zksolc::{
        settings::{Optimizer, ZkSolcError, ZkSolcWarning},
        ZkSettings, ZkSolcCompiler, ZkSolcSettings,
    },
    zksync,
    zksync::artifact_output::zk::{ZkArtifactOutput, ZkContractArtifact},
    ArtifactId, ProjectBuilder, ProjectPathsConfig,
};

#[derive(Debug)]
struct ContractEntry {
    abi: String,
    bytecode: Vec<u8>,
}

impl ContractEntry {
    fn new(artifact: ZkContractArtifact) -> Option<Self> {
        let abi = artifact.abi.expect("no ABI");
        let abi = serde_json::to_string(&abi).expect("cannot serialize ABI to string");
        let bytecode = artifact.bytecode?; // Bytecode is `None` for interfaces
        let bytecode = bytecode
            .object
            .into_bytes()
            .expect("bytecode is not fully compiled")
            .into();
        Some(Self { abi, bytecode })
    }
}

fn resolve_module_name(
    manifest_dir: &Path,
    test_contracts_dir: &Path,
    factory_deps_to_include: &HashSet<String>,
    id: &ArtifactId,
) -> Option<String> {
    let path_in_dir = id.source.strip_prefix(manifest_dir).ok()?;
    let next_dir = path_in_dir.iter().next().expect("no dir");

    let module_name = if next_dir.to_str() == test_contracts_dir.to_str() {
        // We will use the test name directory and not the `contracts` directory for the module
        path_in_dir.iter().nth(1).expect("no dir")
    } else if factory_deps_to_include.contains(&format!(
        "{}:{}",
        path_in_dir.to_str().unwrap(),
        id.name
    )) {
        // We will use the dependency's directory as the module name
        next_dir
    } else {
        return None;
    };

    Some(
        module_name
            .to_str()
            .expect("contract dir is not UTF-8")
            .replace('-', "_"),
    )
}

fn save_artifacts(
    output: &mut impl Write,
    artifacts: impl Iterator<Item = (ArtifactId, ZkContractArtifact)>,
) {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let test_contracts_dir = Path::new("contracts");
    let source_dir = manifest_dir.join(test_contracts_dir);
    let mut modules = HashMap::<_, HashMap<_, _>>::new();

    let artifacts: Vec<_> = artifacts.collect();

    let factory_deps_to_include: HashSet<_> = artifacts
        .iter()
        .filter_map(|(id, artifact)| {
            if !id.source.starts_with(&source_dir) {
                return None; // The artifact doesn't correspond to a source contract
            };

            let Some(factory_deps) = &artifact.factory_dependencies else {
                return None;
            };

            Some(factory_deps.values())
        })
        .flatten()
        .cloned()
        .collect();

    for (id, artifact) in artifacts {
        let Some(module_name) = resolve_module_name(
            manifest_dir,
            test_contracts_dir,
            &factory_deps_to_include,
            &id,
        ) else {
            continue;
        };

        if let Some(entry) = ContractEntry::new(artifact) {
            modules
                .entry(module_name)
                .or_default()
                .insert(id.name, entry);
        }
    }

    for (module_name, module_entries) in modules {
        writeln!(output, "pub(crate) mod {module_name} {{").unwrap();
        for (contract_name, entry) in module_entries {
            writeln!(
                output,
                "    pub(crate) const {contract_name}: crate::contracts::RawContract = crate::contracts::RawContract {{"
            )
            .unwrap();
            writeln!(output, "        abi: r#\"{}\"#,", entry.abi).unwrap(); // ABI shouldn't include '"#' combinations for this to work
            writeln!(output, "        bytecode: &{:?},", entry.bytecode).unwrap();
            writeln!(output, "    }};").unwrap();
        }
        writeln!(output, "}}").unwrap();
    }
}

/// `zksolc` compiler settings.
fn compiler_settings() -> ZkSolcSettings {
    ZkSolcSettings {
        cli_settings: solc::CliSettings::default(),
        settings: ZkSettings {
            // Optimizer must be enabled; otherwise, system calls work incorrectly for whatever reason
            optimizer: Optimizer {
                enabled: Some(true),
                ..Optimizer::default()
            },
            // Required by optimizer
            via_ir: Some(true),
            output_selection: OutputSelection {
                all: FileOutputSelection {
                    per_file: HashSet::from([OutputSelectionFlag::ABI]),
                    per_contract: HashSet::from([OutputSelectionFlag::ABI]),
                },
            },
            enable_eravm_extensions: true,
            suppressed_errors: HashSet::from([ZkSolcError::SendTransfer]),
            suppressed_warnings: HashSet::from([ZkSolcWarning::TxOrigin]),
            ..ZkSettings::default()
        },
    }
}

fn main() {
    let settings = compiler_settings();
    let temp_dir = PathBuf::from(env::var("OUT_DIR").expect("no `OUT_DIR` provided"));
    let paths = ProjectPathsConfig::builder()
        .sources(Path::new(env!("CARGO_MANIFEST_DIR")).join("contracts"))
        .remapping(Remapping {
            context: None,
            name: "@openzeppelin/contracts-v4".into(),
            path: format!(
                "{}/contract-libs/openzeppelin-contracts-v4/contracts",
                env!("CARGO_MANIFEST_DIR")
            ),
        })
        .remapping(Remapping {
            context: None,
            name: "l1-contracts".into(),
            path: format!(
                "{}/contract-libs/l1-contracts/contracts",
                env!("CARGO_MANIFEST_DIR")
            ),
        })
        .remapping(Remapping {
            context: None,
            name: "@openzeppelin/contracts-upgradeable-v4".into(),
            path: format!(
                "{}/contract-libs/openzeppelin-contracts-upgradeable-v4/contracts",
                env!("CARGO_MANIFEST_DIR")
            ),
        })
        .artifacts(temp_dir.join("artifacts"))
        .cache(temp_dir.join("cache"))
        .build()
        .unwrap();

    let project = ProjectBuilder::<ZkSolcCompiler, _>::new(ZkArtifactOutput::default())
        .paths(paths)
        .settings(settings)
        .build(ZkSolcCompiler::default())
        .unwrap();
    let output = zksync::project_compile(&project).unwrap();
    output.assert_success();

    let module_path = temp_dir.join("raw_contracts.rs");
    let module = File::create(&module_path).expect("failed creating output Rust module");
    let mut module = BufWriter::new(module);
    save_artifacts(&mut module, output.into_artifacts());

    // Tell Cargo that if a source file changes, to rerun this build script.
    project.rerun_if_sources_changed();
}
