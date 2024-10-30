use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use xshell::{cmd, Shell};

const IGNORE_FILE: &str = "etc/lint-config/ignore.yaml";

#[derive(Debug, ValueEnum, EnumIter, strum::Display, PartialEq, Eq, Clone, Copy)]
#[strum(serialize_all = "lowercase")]
pub enum Target {
    Md,
    Sol,
    Js,
    Ts,
    Rs,
    Contracts,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct IgnoredData {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
}

impl IgnoredData {
    pub fn merge(&mut self, other: &IgnoredData) {
        self.files.extend(other.files.clone());
        self.dirs.extend(other.dirs.clone());
    }
}

pub fn get_unignored_files(
    shell: &Shell,
    target: &Target,
    ignored_data: Option<IgnoredData>,
) -> anyhow::Result<Vec<String>> {
    let mut files = Vec::new();
    let mut ignored_files: IgnoredData = serde_yaml::from_str(&shell.read_file(IGNORE_FILE)?)?;
    if let Some(ignored_data) = ignored_data {
        ignored_files.merge(&ignored_data);
    }
    let output = cmd!(shell, "git ls-files --recurse-submodules").read()?;

    for line in output.lines() {
        let path = line.to_string();
        if !ignored_files.dirs.iter().any(|dir| path.contains(dir))
            && !ignored_files.files.contains(&path)
            && path.ends_with(&format!(".{}", target))
        {
            files.push(path);
        }
    }

    Ok(files)
}
