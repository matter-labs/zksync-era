use envy::prefixed;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
pub struct CheckerConfig {
    pub mode: Mode,
    pub start_miniblock: Option<u32>,
    pub finish_miniblock: Option<u32>,
    pub main_node_url: String,
    pub instances_urls: Vec<String>,
    pub instance_poll_period: u64,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub enum Mode {
    Triggered,
    Continuous,
}

impl CheckerConfig {
    pub fn from_env() -> Self {
        prefixed("CHECKER_")
            .from_env()
            .unwrap_or_else(|err| panic!("Failed to load the checker config with error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn success() {
        let config = r#"
            CHECKER_MODE="Continuous"
            CHECKER_START_MINIBLOCK="2"
            CHECKER_FINISH_MINIBLOCK="4"
            CHECKER_MAIN_NODE_URL="http://127.0.0.1:1020"
            CHECKER_INSTANCES_URLS="http://127.0.0.1:1030,http://127.0.0.1:1020"
            CHECKER_INSTANCE_POLL_PERIOD="60"
        "#;

        set_env(config);

        let actual = CheckerConfig::from_env();
        let want = CheckerConfig {
            mode: Mode::Continuous,
            start_miniblock: Some(2),
            finish_miniblock: Some(4),
            main_node_url: "http://127.0.0.1:1020".into(),
            instances_urls: vec![
                "http://127.0.0.1:1030".into(),
                "http://127.0.0.1:1020".into(),
            ],
            instance_poll_period: 60,
        };
        assert_eq!(actual, want);
    }

    pub fn set_env(fixture: &str) {
        for line in fixture.split('\n').map(str::trim) {
            if line.is_empty() {
                continue;
            }
            let elements: Vec<_> = line.split('=').collect();
            let variable_name = elements[0];
            let variable_value = elements[1].trim_matches('"');

            env::set_var(variable_name, variable_value);
        }
    }
}
