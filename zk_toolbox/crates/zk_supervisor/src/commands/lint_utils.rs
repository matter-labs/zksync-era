use clap::ValueEnum;
use strum::EnumIter;
use xshell::{cmd, Shell};

const IGNORED_DIRS: [&str; 18] = [
    "target",
    "node_modules",
    "volumes",
    "build",
    "dist",
    ".git",
    "generated",
    "grafonnet-lib",
    "prettier-config",
    "lint-config",
    "cache",
    "artifacts",
    "typechain",
    "binaryen",
    "system-contracts",
    "artifacts-zk",
    "cache-zk",
    // Ignore directories with OZ and forge submodules.
    "contracts/l1-contracts/lib",
];

const IGNORED_FILES: [&str; 4] = [
    "KeysWithPlonkVerifier.sol",
    "TokenInit.sol",
    ".tslintrc.js",
    ".prettierrc.js",
];

#[derive(Debug, ValueEnum, EnumIter, strum::Display, PartialEq, Eq, Clone, Copy)]
#[strum(serialize_all = "lowercase")]
pub enum Extension {
    Md,
    Sol,
    Js,
    Ts,
    Rs,
}

pub fn get_unignored_files(shell: &Shell, extension: &Extension) -> anyhow::Result<Vec<String>> {
    let mut files = Vec::new();
    let output = cmd!(shell, "git ls-files").read()?;

    for line in output.lines() {
        let path = line.to_string();
        if !IGNORED_DIRS.iter().any(|dir| path.contains(dir))
            && !IGNORED_FILES.contains(&path.as_str())
            && path.ends_with(&format!(".{}", extension))
        {
            files.push(path);
        }
    }

    Ok(files)
}
