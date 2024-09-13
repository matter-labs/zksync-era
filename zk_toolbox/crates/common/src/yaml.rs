use anyhow::Context;

use crate::logger;

pub(super) const MSG_INVALID_KEY_TYPE_ERR: &str = "Invalid key type";

/// Holds the differences between two YAML configurations.
#[derive(Default)]
pub struct ConfigDiff {
    /// Fields that have different values between the two configurations
    /// This contains the new values
    pub differing_values: serde_yaml::Mapping,

    /// Fields that are present in the new configuration but not in the old one.
    pub new_fields: serde_yaml::Mapping,
}

impl ConfigDiff {
    pub fn print(&self, msg: &str, is_warning: bool) {
        if self.new_fields.is_empty() {
            return;
        }

        if is_warning {
            logger::warn(msg);
            logger::warn(logger::object_to_string(&self.new_fields));
        } else {
            logger::info(msg);
            logger::info(logger::object_to_string(&self.new_fields));
        }
    }
}

fn merge_yaml_internal(
    a: &mut serde_yaml::Value,
    b: serde_yaml::Value,
    current_key: String,
    diff: &mut ConfigDiff,
    override_values: bool,
) -> anyhow::Result<()> {
    match (a, b) {
        (serde_yaml::Value::Mapping(a), serde_yaml::Value::Mapping(b)) => {
            for (key, value) in b {
                let k = key.as_str().context(MSG_INVALID_KEY_TYPE_ERR)?.to_string();
                let current_key = if current_key.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", current_key, k)
                };

                if a.contains_key(&key) {
                    let a_value = a.get_mut(&key).unwrap();
                    if value.is_null() && override_values {
                        a.remove(&key);
                        diff.differing_values
                            .insert(current_key.into(), serde_yaml::Value::Null);
                    } else {
                        merge_yaml_internal(a_value, value, current_key, diff, override_values)?;
                    }
                } else if !value.is_null() {
                    a.insert(key.clone(), value.clone());
                    diff.new_fields.insert(current_key.into(), value);
                } else if override_values {
                    diff.differing_values
                        .insert(current_key.into(), serde_yaml::Value::Null);
                }
            }
        }
        (a, b) => {
            if a != &b {
                diff.differing_values.insert(current_key.into(), b.clone());
                if override_values {
                    *a = b;
                }
            }
        }
    }
    Ok(())
}

pub fn merge_yaml(
    a: &mut serde_yaml::Value,
    b: serde_yaml::Value,
    override_values: bool,
) -> anyhow::Result<ConfigDiff> {
    let mut diff = ConfigDiff::default();
    merge_yaml_internal(a, b, "".into(), &mut diff, override_values)?;
    Ok(diff)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_merge_yaml_both_are_equal_returns_no_diff() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
        key1: value1
        key2: value2
        key3:
            key4: value4
        "#,
        )
        .unwrap();
        let diff = super::merge_yaml(&mut a, b, false).unwrap();
        assert!(diff.differing_values.is_empty());
        assert!(diff.new_fields.is_empty());
        assert_eq!(a, expected);
    }

    #[test]
    fn test_merge_yaml_b_has_extra_field_returns_diff() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            "#,
        )
        .unwrap();

        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            "#,
        )
        .unwrap();

        let diff = super::merge_yaml(&mut a, b.clone(), false).unwrap();
        assert!(diff.differing_values.is_empty());
        assert_eq!(diff.new_fields.len(), 1);
        assert_eq!(
            diff.new_fields.get::<String>("key5".into()).unwrap(),
            b.clone().get("key5").unwrap()
        );
        assert_eq!(a, expected);
    }

    #[test]
    fn test_merge_yaml_a_has_extra_field_no_diff() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();

        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            "#,
        )
        .unwrap();

        let diff = super::merge_yaml(&mut a, b, false).unwrap();
        assert!(diff.differing_values.is_empty());
        assert!(diff.new_fields.is_empty());
        assert_eq!(a, expected);
    }

    #[test]
    fn test_merge_yaml_a_has_extra_field_and_b_has_extra_field_returns_diff() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key6: value6
            "#,
        )
        .unwrap();

        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            key6: value6
            "#,
        )
        .unwrap();

        let diff = super::merge_yaml(&mut a, b.clone(), false).unwrap();
        assert_eq!(diff.differing_values.len(), 0);
        assert_eq!(diff.new_fields.len(), 1);
        assert_eq!(
            diff.new_fields.get::<String>("key6".into()).unwrap(),
            b.clone().get("key6").unwrap()
        );
        assert_eq!(a, expected);
    }

    #[test]
    fn test_merge_yaml_a_has_different_value_returns_diff() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value5
            "#,
        )
        .unwrap();

        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();

        let diff = super::merge_yaml(&mut a, b.clone(), false).unwrap();
        assert_eq!(diff.differing_values.len(), 1);
        assert_eq!(
            diff.differing_values
                .get::<serde_yaml::Value>("key3.key4".into())
                .unwrap(),
            b.get("key3").unwrap().get("key4").unwrap()
        );
        assert_eq!(a, expected);
    }

    #[test]
    fn test_merge_yaml_a_has_different_value_and_b_has_extra_field_returns_diff() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value5
            key5: value5
            "#,
        )
        .unwrap();

        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            "#,
        )
        .unwrap();

        let diff = super::merge_yaml(&mut a, b.clone(), false).unwrap();
        assert_eq!(diff.differing_values.len(), 1);
        assert_eq!(
            diff.differing_values
                .get::<serde_yaml::Value>("key3.key4".into())
                .unwrap(),
            b.get("key3").unwrap().get("key4").unwrap()
        );
        assert_eq!(diff.new_fields.len(), 1);
        assert_eq!(
            diff.new_fields.get::<String>("key5".into()).unwrap(),
            b.get("key5").unwrap()
        );
        assert_eq!(a, expected);
    }

    #[test]
    fn test_merge_yaml_override_values() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value5
            "#,
        )
        .unwrap();

        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value5
            "#,
        )
        .unwrap();

        let diff = super::merge_yaml(&mut a, b.clone(), true).unwrap();
        assert_eq!(diff.differing_values.len(), 1);
        assert_eq!(
            diff.differing_values
                .get::<serde_yaml::Value>("key3.key4".into())
                .unwrap(),
            b.get("key3").unwrap().get("key4").unwrap()
        );
        assert_eq!(a, expected);
    }

    #[test]
    fn test_merge_yaml_override_values_with_extra_field() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value5
            key5: value5
            "#,
        )
        .unwrap();

        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value5
            key5: value5
            "#,
        )
        .unwrap();

        let diff = super::merge_yaml(&mut a, b.clone(), true).unwrap();
        assert_eq!(diff.differing_values.len(), 1);
        assert_eq!(
            diff.differing_values
                .get::<serde_yaml::Value>("key3.key4".into())
                .unwrap(),
            b.get("key3").unwrap().get("key4").unwrap()
        );
        assert_eq!(diff.new_fields.len(), 1);
        assert_eq!(
            diff.new_fields.get::<String>("key5".into()).unwrap(),
            b.get("key5").unwrap()
        );
        assert_eq!(a, expected);
    }

    #[test]
    fn test_override_values_with_null() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3: null
            "#,
        )
        .unwrap();

        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            "#,
        )
        .unwrap();

        let diff = super::merge_yaml(&mut a, b.clone(), true).unwrap();
        assert_eq!(diff.differing_values.len(), 1);
        assert_eq!(
            diff.differing_values
                .get::<serde_yaml::Value>("key3".into())
                .unwrap(),
            b.get("key3").unwrap()
        );
        assert_eq!(a, expected);
    }
}
