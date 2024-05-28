use once_cell::sync::OnceCell;

static CONFIG: OnceCell<GlobalConfig> = OnceCell::new();

pub fn init_global_config(config: GlobalConfig) {
    CONFIG.set(config).unwrap();
}

pub fn global_config() -> &'static GlobalConfig {
    CONFIG.get().expect("GlobalConfig not initialized")
}

#[derive(Debug)]
pub struct GlobalConfig {
    pub verbose: bool,
    pub chain_name: Option<String>,
    pub ignore_prerequisites: bool,
}
