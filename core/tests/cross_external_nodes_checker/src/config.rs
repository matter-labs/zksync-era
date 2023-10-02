use envy::prefixed;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
pub struct CheckerConfig {
    #[serde(default = "default_mode")]
    pub mode: Mode,

    #[serde(default = "default_rpc_mode")]
    pub rpc_mode: Option<RpcMode>,

    #[serde(default = "default_start_miniblock")]
    pub start_miniblock: Option<u32>,

    #[serde(default = "default_finish_miniblock")]
    pub finish_miniblock: Option<u32>,

    #[serde(default = "default_main_node_http_url")]
    pub main_node_http_url: Option<String>,

    #[serde(default = "default_instances_http_urls")]
    pub instances_http_urls: Option<Vec<String>>,

    #[serde(default = "default_main_node_ws_url")]
    pub main_node_ws_url: Option<String>,

    #[serde(default = "default_instances_ws_urls")]
    pub instances_ws_urls: Option<Vec<String>>,

    #[serde(default = "default_max_transactions_to_check")]
    pub max_transactions_to_check: Option<usize>,

    #[serde(default = "default_instance_poll_period")]
    pub instance_poll_period: Option<u64>,

    #[serde(default = "default_subscription_duration")]
    pub subscription_duration: Option<u64>,
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq)]
pub enum Mode {
    Rpc,
    PubSub,
    All,
}

impl Mode {
    pub fn run_rpc(&self) -> bool {
        matches!(self, Mode::Rpc | Mode::All)
    }

    pub fn run_pubsub(&self) -> bool {
        matches!(self, Mode::PubSub | Mode::All)
    }
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq)]
pub enum RpcMode {
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

// Default functions for each of the fields

fn default_mode() -> Mode {
    Mode::All
}

fn default_rpc_mode() -> Option<RpcMode> {
    Some(RpcMode::Triggered)
}

fn default_start_miniblock() -> Option<u32> {
    None
}

fn default_finish_miniblock() -> Option<u32> {
    None
}

fn default_main_node_http_url() -> Option<String> {
    Some("http://127.0.0.1:3050".to_string())
}

fn default_instances_http_urls() -> Option<Vec<String>> {
    Some(vec!["http://127.0.0.1:3060".to_string()])
}

fn default_main_node_ws_url() -> Option<String> {
    Some("ws://127.0.0.1:3051".to_string())
}

fn default_instances_ws_urls() -> Option<Vec<String>> {
    Some(vec!["ws://127.0.0.1:3061".to_string()])
}

fn default_max_transactions_to_check() -> Option<usize> {
    Some(3)
}

fn default_instance_poll_period() -> Option<u64> {
    Some(10)
}

fn default_subscription_duration() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn success() {
        let config = r#"
            CHECKER_MODE="Rpc"
            CHECKER_RPC_MODE="Continuous"
            CHECKER_START_MINIBLOCK="2"
            CHECKER_FINISH_MINIBLOCK="4"
            CHECKER_MAIN_NODE_HTTP_URL="http://127.0.0.1:1020"
            CHECKER_INSTANCES_HTTP_URLS="http://127.0.0.1:1030,http://127.0.0.1:1020"
            CHECKER_INSTANCE_POLL_PERIOD="60"
            CHECKER_MAX_TRANSACTIONS_TO_CHECK="10"
            CHECKER_SUBSCRIPTION_DURATION="120"
        "#;

        set_env(config);

        let actual = CheckerConfig::from_env();
        let want = CheckerConfig {
            mode: Mode::Rpc,
            rpc_mode: Some(RpcMode::Continuous),
            start_miniblock: Some(2),
            finish_miniblock: Some(4),
            main_node_http_url: Some("http://127.0.0.1:1020".into()),
            instances_http_urls: Some(vec![
                "http://127.0.0.1:1030".into(),
                "http://127.0.0.1:1020".into(),
            ]),
            main_node_ws_url: Some("ws://127.0.0.1:3051".into()),
            instances_ws_urls: Some(vec!["ws://127.0.0.1:3061".into()]),
            instance_poll_period: Some(60),
            max_transactions_to_check: Some(10),
            subscription_duration: Some(120),
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
