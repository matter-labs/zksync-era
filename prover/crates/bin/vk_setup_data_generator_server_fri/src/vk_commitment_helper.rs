use std::{fs, path::PathBuf};

use anyhow::Context as _;
use toml_edit::{Document, Item, Value};
use zksync_utils::env::Workspace;

pub fn get_toml_formatted_value(string_value: String) -> Item {
    let mut value = Value::from(string_value);
    value.decor_mut().set_prefix("");
    Item::Value(value)
}

pub fn write_contract_toml(contract_doc: Document) -> anyhow::Result<()> {
    let path = get_contract_toml_path();
    fs::write(path, contract_doc.to_string()).context("Failed writing to contract.toml file")
}

pub fn read_contract_toml() -> anyhow::Result<Document> {
    let path = get_contract_toml_path();
    let toml_data = std::fs::read_to_string(path.clone())
        .with_context(|| format!("contract.toml file does not exist on path {path:?}"))?;
    toml_data.parse::<Document>().context("invalid config file")
}

pub fn get_contract_toml_path() -> PathBuf {
    Workspace::locate()
        .root()
        .join("etc/env/base/contracts.toml")
}
