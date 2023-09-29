use std::fs;
use toml_edit::{Document, Item, Value};

pub fn get_toml_formatted_value(string_value: String) -> Item {
    let mut value = Value::from(string_value);
    value.decor_mut().set_prefix("");
    Item::Value(value)
}

pub fn write_contract_toml(contract_doc: Document) {
    let path = get_contract_toml_path();
    fs::write(path, contract_doc.to_string()).expect("Failed writing to contract.toml file");
}

pub fn read_contract_toml() -> Document {
    let path = get_contract_toml_path();
    let toml_data = std::fs::read_to_string(path.clone())
        .unwrap_or_else(|_| panic!("contract.toml file does not exist on path {}", path));
    toml_data.parse::<Document>().expect("invalid config file")
}

pub fn get_contract_toml_path() -> String {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| "/".into());
    format!("{}/etc/env/base/contracts.toml", zksync_home)
}
