use zksync_basic_types::H256;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CommitmentKeys {
    pub snark_wrapper: H256,
    /// Other settings (only filled when parsing `StandardJson` input from the request).
    #[serde(flatten)]
    other: serde_json::Value,
}

impl CommitmentKeys {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let file = std::fs::File::open(path)?;
        let keys: Self = serde_json::from_reader(file)?;
        Ok(keys)
    }

    pub fn save_to_file(&self, path: &str) -> anyhow::Result<()> {
        let file = std::fs::File::create(path)?;
        serde_json::to_writer(file, self)?;
        Ok(())
    }
}
