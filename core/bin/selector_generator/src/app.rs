use std::path::PathBuf;

use anyhow::Context;
use glob::glob;
use tokio::io::AsyncWriteExt as _;

use crate::selectors::Selectors;

#[derive(Debug, Default)]
pub(crate) struct App {
    /// Selectors file.
    file_path: PathBuf,
    /// All the selectors. Initially, will be loaded from the file.
    /// All the discovered selectors will be merged into it.
    selectors: Selectors,
    /// Number of selectors before processing the files.
    /// Used for reporting.
    selectors_before: usize,
    /// Number of files analyzed.
    /// Used for reporting.
    analyzed_files: usize,
}

impl App {
    /// Loads the selectors from the file, or returns a new instance if the file doesn't exist.
    pub async fn load(file_path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let file_path = file_path.into();
        // If doesn't exist, return default.
        if !file_path.exists() {
            return Ok(Self::default());
        }

        let file = tokio::fs::read(&file_path)
            .await
            .context("Failed to read file")?;
        let selectors: Selectors =
            serde_json::from_slice(&file).context("Failed to deserialize file")?;
        let selectors_before = selectors.len();
        Ok(Self {
            file_path,
            selectors,
            selectors_before,
            analyzed_files: 0,
        })
    }

    /// Analyses all the JSON files, looking for 'abi' entries, and then computing the selectors for them.
    pub async fn process_files(&mut self, directory: &str) -> anyhow::Result<()> {
        for file_path in Self::load_file_paths(directory) {
            let Ok(new_selectors) = Selectors::load(&file_path).await.inspect_err(|e| {
                eprintln!("Error parsing file {file_path:?}: {e:?}");
            }) else {
                continue;
            };
            self.merge(new_selectors);
        }
        Ok(())
    }

    /// Saves the selectors to the file.
    pub async fn save(self) -> anyhow::Result<()> {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.file_path)
            .await
            .context("Failed to open file")?;
        let json = serde_json::to_string_pretty(&self.selectors)?;
        file.write_all(json.as_bytes())
            .await
            .context("Failed to save file")?;
        Ok(())
    }

    /// Merges the new selectors into the current ones.
    pub fn merge(&mut self, new: Selectors) {
        self.selectors.merge(new);
        self.analyzed_files += 1;
    }

    /// Reports the number of analyzed files and the number of added selectors.
    pub fn report(&self) {
        println!(
            "Analyzed {} files. Added {} selectors (before: {} after: {})",
            self.analyzed_files,
            self.selectors.len() - self.selectors_before,
            self.selectors_before,
            self.selectors.len()
        );
    }

    fn load_file_paths(dir: &str) -> Vec<PathBuf> {
        glob(&format!("{}/**/*.json", dir))
            .expect("Failed to read glob pattern")
            .filter_map(|entry| match entry {
                Ok(path) => Some(path),
                Err(e) => {
                    eprintln!("Error reading file: {:?}", e);
                    None
                }
            })
            .collect()
    }
}
