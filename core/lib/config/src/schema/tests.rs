//! Self-contained tests for configuration schema.

use serde::Deserialize;

use super::{
    env::{EnvParserEngine, Environment},
    *,
};
use crate::metadata::DescribeConfig;

struct MockEngineAssertion {
    prefix: &'static str,
    expected_vars: HashMap<String, String>,
    value_producer: fn() -> Box<dyn any::Any>,
}

#[derive(Default)]
struct MockEngine {
    assertions: HashMap<any::TypeId, MockEngineAssertion>,
}

impl MockEngine {
    fn insert<C: DescribeConfig + Default>(
        &mut self,
        prefix: &'static str,
        expected_vars: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) {
        let assertion = MockEngineAssertion {
            prefix,
            expected_vars: expected_vars
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
            value_producer: || Box::<C>::default() as Box<dyn any::Any>,
        };
        self.assertions.insert(any::TypeId::of::<C>(), assertion);
    }
}

impl EnvParserEngine for MockEngine {
    fn parse_from_env<C: DescribeConfig>(
        &self,
        prefix: &str,
        vars: &HashMap<String, String>,
    ) -> anyhow::Result<C> {
        let assertion = &self.assertions[&any::TypeId::of::<C>()];
        assert_eq!(assertion.prefix, prefix);
        assert_eq!(assertion.expected_vars, *vars);
        Ok(*(assertion.value_producer)().downcast::<C>().unwrap())
    }
}

/// # Test configuration
///
/// Extended description.
#[derive(Debug, Default, PartialEq, Deserialize, DescribeConfig)]
#[config(crate = crate)]
struct TestConfig {
    /// String value.
    #[serde(alias = "string", default = "TestConfig::default_str")]
    str: String,
    /// Optional value.
    #[serde(rename = "optional")]
    optional_int: Option<u32>,
}

impl TestConfig {
    fn default_str() -> String {
        "default".to_owned()
    }
}

#[test]
fn getting_config_metadata() {
    let metadata = TestConfig::describe_config();
    assert_eq!(metadata.ty.name_in_code(), "TestConfig");
    assert_eq!(metadata.help, "# Test configuration\nExtended description.");
    assert_eq!(metadata.help_header(), Some("Test configuration"));
    assert_eq!(metadata.params.len(), 2);

    let str_metadata = &metadata.params[0];
    assert_eq!(str_metadata.name, "str");
    assert_eq!(str_metadata.aliases, ["string"]);
    assert_eq!(str_metadata.help, "String value.");
    assert_eq!(str_metadata.ty.name_in_code(), "String");
    assert_eq!(
        format!("{:?}", str_metadata.default_value().unwrap()),
        "\"default\""
    );

    let optional_metadata = &metadata.params[1];
    assert_eq!(optional_metadata.name, "optional");
    assert_eq!(optional_metadata.aliases, [] as [&str; 0]);
    assert_eq!(optional_metadata.help, "Optional value.");
    assert_eq!(optional_metadata.ty.name_in_code(), "Option<u32>");
    assert_eq!(optional_metadata.base_type.name_in_code(), "u32");
}

const EXPECTED_HELP: &str = r#"
# Test configuration
Extended description.

str
string
    Type: string [Rust: String], default: "default"
    String value.

optional
    Type: integer [Rust: Option<u32>], default: None
    Optional value.
"#;

#[test]
fn printing_schema_help() {
    let schema = ConfigSchema::default().insert::<TestConfig>("");
    let mut buffer = vec![];
    schema.write_help(&mut buffer, |_| true).unwrap();
    let buffer = String::from_utf8(buffer).unwrap();
    assert_eq!(buffer.trim(), EXPECTED_HELP.trim(), "{buffer}");
}

#[test]
fn using_alias() {
    let schema = ConfigSchema::default().insert_aliased::<TestConfig>("test", [Alias::prefix("")]);
    let env = Environment::from_iter("APP_", [("APP_TEST_STR", "test"), ("APP_OPTIONAL", "123")]);

    let mut mock_engine = MockEngine::default();
    mock_engine.insert::<TestConfig>(
        "TEST_",
        [
            ("TEST_STR", "test"),
            ("OPTIONAL", "123"),
            ("TEST_OPTIONAL", "123"),
        ],
    );
    env.parser(mock_engine, &schema)
        .parse::<TestConfig>()
        .unwrap();
}

#[test]
fn using_multiple_aliases() {
    let schema = ConfigSchema::default().insert_aliased::<TestConfig>(
        "test",
        [
            Alias::prefix("").exclude(|name| name == "optional"),
            Alias::prefix("deprecated"),
        ],
    );
    let env = Environment::from_iter(
        "APP_",
        [
            ("APP_TEST_STR", "?"),
            ("APP_OPTIONAL", "123"), // should not be used (excluded from alias)
            ("APP_DEPRECATED_STR", "!"), // should not be used (original var is defined)
            ("APP_DEPRECATED_OPTIONAL", "321"),
        ],
    );

    let mut mock_engine = MockEngine::default();
    mock_engine.insert::<TestConfig>(
        "TEST_",
        [
            ("TEST_STR", "?"),
            ("OPTIONAL", "123"),
            ("DEPRECATED_STR", "!"),
            ("DEPRECATED_OPTIONAL", "321"),
            ("TEST_OPTIONAL", "321"),
        ],
    );
    env.parser(mock_engine, &schema)
        .parse::<TestConfig>()
        .unwrap();
}
