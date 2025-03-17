use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env,
    fs::File,
    io::{BufWriter, Write},
    iter,
    path::{Path, PathBuf},
};

use foundry_compilers::{
    artifacts::{
        zksolc::output_selection::{FileOutputSelection, OutputSelection, OutputSelectionFlag},
        ConfigurableContractArtifact, Remapping, Settings,
    },
    solc,
    solc::{SolcCompiler, SolcLanguage, SolcSettings},
    zksolc::{ZkSettings, ZkSolcCompiler, ZkSolcSettings},
    zksync,
    zksync::artifact_output::zk::{ZkArtifactOutput, ZkContractArtifact},
    ArtifactId, ConfigurableArtifacts, ProjectBuilder, ProjectPathsConfig,
};

trait ContractEntry: Sized {
    type Artifact;

    fn source_dir() -> &'static Path;

    fn factory_deps(artifact: &Self::Artifact) -> impl Iterator<Item = &str> + '_;

    fn from_raw(raw: Self::Artifact) -> Option<Self>;

    fn write(&self, writer: &mut impl Write, name: &str);
}

#[derive(Debug)]
struct EravmContractEntry {
    abi: String,
    bytecode: Vec<u8>,
}

impl ContractEntry for EravmContractEntry {
    type Artifact = ZkContractArtifact;

    fn source_dir() -> &'static Path {
        Path::new("contracts")
    }

    fn factory_deps(artifact: &Self::Artifact) -> impl Iterator<Item = &str> + '_ {
        const EMPTY_DEPS: &BTreeMap<String, String> = &BTreeMap::new();

        let factory_deps = artifact.factory_dependencies.as_ref().unwrap_or(EMPTY_DEPS);
        factory_deps.values().map(String::as_str)
    }

    fn from_raw(artifact: Self::Artifact) -> Option<Self> {
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

    fn write(&self, output: &mut impl Write, name: &str) {
        writeln!(
            output,
            "    pub(crate) const {name}: crate::contracts::RawContract = crate::contracts::RawContract {{"
        )
        .unwrap();
        writeln!(output, "        abi: r#\"{}\"#,", self.abi).unwrap(); // ABI shouldn't include '"#' combinations for this to work
        writeln!(output, "        bytecode: &{:?},", self.bytecode).unwrap();
        writeln!(output, "    }};").unwrap();
    }
}

#[derive(Debug)]
struct EvmContractEntry {
    abi: String,
    init_bytecode: Vec<u8>,
    deployed_bytecode: Vec<u8>,
}

impl ContractEntry for EvmContractEntry {
    type Artifact = ConfigurableContractArtifact;

    fn source_dir() -> &'static Path {
        Path::new("evm-contracts")
    }

    fn factory_deps(_artifact: &Self::Artifact) -> impl Iterator<Item = &str> + '_ {
        iter::empty()
    }

    fn from_raw(artifact: Self::Artifact) -> Option<Self> {
        let abi = artifact.abi.expect("no ABI");
        let abi = serde_json::to_string(&abi).expect("cannot serialize ABI to string");
        let init_bytecode = artifact.bytecode?;
        let deployed_bytecode = artifact.deployed_bytecode?.bytecode?;

        let init_bytecode: Vec<u8> = init_bytecode
            .object
            .into_bytes()
            .expect("bytecode is not fully compiled")
            .into();
        if init_bytecode.is_empty() {
            return None;
        }
        let deployed_bytecode: Vec<u8> = deployed_bytecode
            .object
            .into_bytes()
            .expect("bytecode is not fully compiled")
            .into();
        if deployed_bytecode.is_empty() {
            return None;
        }

        Some(Self {
            abi,
            init_bytecode,
            deployed_bytecode,
        })
    }

    fn write(&self, output: &mut impl Write, name: &str) {
        writeln!(
            output,
            "    pub(crate) const {name}: crate::contracts::RawEvmContract = crate::contracts::RawEvmContract {{"
        )
        .unwrap();
        writeln!(output, "        abi: r#\"{}\"#,", self.abi).unwrap(); // ABI shouldn't include '"#' combinations for this to work
        writeln!(output, "        init_bytecode: &{:?},", self.init_bytecode).unwrap();
        writeln!(
            output,
            "        deployed_bytecode: &{:?},",
            self.deployed_bytecode
        )
        .unwrap();
        writeln!(output, "    }};").unwrap();
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

    let mut module_name = module_name
        .to_str()
        .expect("contract dir is not UTF-8")
        .replace('-', "_");
    if module_name.ends_with(".sol") {
        module_name.truncate(module_name.len() - 4);
    }
    Some(module_name)
}

fn save_artifacts<E: ContractEntry>(
    output: &mut impl Write,
    artifacts: impl Iterator<Item = (ArtifactId, E::Artifact)>,
) {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let test_contracts_dir = E::source_dir();
    let source_dir = manifest_dir.join(test_contracts_dir);
    let mut modules = HashMap::<_, HashMap<_, _>>::new();

    let artifacts: Vec<_> = artifacts.collect();

    let factory_deps_to_include: HashSet<_> = artifacts
        .iter()
        .filter_map(|(id, artifact)| {
            if !id.source.starts_with(&source_dir) {
                return None; // The artifact doesn't correspond to a source contract
            };
            Some(E::factory_deps(artifact))
        })
        .flatten()
        .map(str::to_owned)
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

        if let Some(entry) = E::from_raw(artifact) {
            modules
                .entry(module_name)
                .or_default()
                .insert(id.name, entry);
        }
    }

    for (module_name, module_entries) in modules {
        writeln!(output, "pub(crate) mod {module_name} {{").unwrap();
        for (contract_name, entry) in module_entries {
            entry.write(output, &contract_name);
        }
        writeln!(output, "}}").unwrap();
    }
}

/// `zksolc` compiler settings.
fn zksolc_settings() -> ZkSolcSettings {
    use foundry_compilers::zksolc::settings::{Optimizer, ZkSolcError, ZkSolcWarning};

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

fn compile_eravm_contracts(temp_dir: &Path) {
    let settings = zksolc_settings();
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
    save_artifacts::<EravmContractEntry>(&mut module, output.into_artifacts());

    // Tell Cargo that if a source file changes, to rerun this build script.
    project.rerun_if_sources_changed();
}

fn solc_settings() -> SolcSettings {
    use foundry_compilers::artifacts::Optimizer;

    SolcSettings {
        settings: Settings {
            optimizer: Optimizer {
                enabled: Some(true),
                ..Optimizer::default()
            },
            ..Settings::default()
        },
        ..SolcSettings::default()
    }
}

fn compile_evm_contracts(temp_dir: &Path) {
    let paths = ProjectPathsConfig::builder()
        .sources(Path::new(env!("CARGO_MANIFEST_DIR")).join("evm-contracts"))
        .artifacts(temp_dir.join("evm-artifacts"))
        .cache(temp_dir.join("evm-cache"))
        .build::<SolcLanguage>()
        .unwrap();

    let project = ProjectBuilder::new(ConfigurableArtifacts::default())
        .paths(paths)
        .settings(solc_settings())
        .build(SolcCompiler::default())
        .unwrap();
    let output = project.compile().unwrap();
    output.assert_success();

    let module_path = temp_dir.join("raw_evm_contracts.rs");
    let module = File::create(&module_path).expect("failed creating output Rust module");
    let mut module = BufWriter::new(module);
    save_artifacts::<EvmContractEntry>(&mut module, output.into_artifacts());

    project.rerun_if_sources_changed();
}

fn main() {
    let temp_dir = PathBuf::from(env::var("OUT_DIR").expect("no `OUT_DIR` provided"));
    compile_eravm_contracts(&temp_dir);
    compile_evm_contracts(&temp_dir);
}
