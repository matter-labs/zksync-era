use futures::channel::oneshot;
use serde_json::{Map, Value};
use std::future::Future;
use std::{collections::HashMap, time::Duration};
use tokio::time::sleep;

/// Sets up an interrupt handler and returns a future that resolves once an interrupt signal is received.
pub fn setup_sigint_handler() -> oneshot::Receiver<()> {
    let (sigint_sender, sigint_receiver) = oneshot::channel();
    let mut sigint_sender = Some(sigint_sender);
    ctrlc::set_handler(move || {
        if let Some(sigint_sender) = sigint_sender.take() {
            sigint_sender.send(()).ok();
            // ^ The send fails if `sigint_receiver` is dropped. We're OK with this,
            // since at this point the node should be stopping anyway, or is not interested
            // in listening to interrupt signals.
        }
    })
    .expect("Error setting Ctrl+C handler");

    sigint_receiver
}

pub fn compare_json<T: serde::Serialize>(
    a: &T,
    b: &T,
    path: String,
) -> HashMap<String, (Option<Value>, Option<Value>)> {
    let a = serde_json::to_value(a).expect("serialization failure");
    let b = serde_json::to_value(b).expect("serialization failure");

    if a == b {
        return HashMap::new();
    }

    match (a, b) {
        (Value::Object(ref a), Value::Object(ref b)) => compare_json_object(a, b, path),
        (Value::Array(ref a), Value::Array(ref b)) => compare_json_array(a, b, path),
        (a, b) => {
            let mut res = HashMap::new();
            let a_val = if a.is_null() { None } else { Some(a) };
            let b_val = if b.is_null() { None } else { Some(b) };
            res.insert(path, (a_val, b_val));
            res
        }
    }
}

fn compare_json_object(
    a: &Map<String, Value>,
    b: &Map<String, Value>,
    path: String,
) -> HashMap<String, (Option<Value>, Option<Value>)> {
    let mut differences = HashMap::new();

    for (k, v) in a.iter() {
        let new_path = if path.is_empty() {
            k.clone()
        } else {
            format!("{}.{}", path, k)
        };

        differences.extend(compare_json(v, b.get(k).unwrap_or(&Value::Null), new_path));
    }

    for (k, v) in b.iter() {
        if !a.contains_key(k) {
            let new_path = if path.is_empty() {
                k.clone()
            } else {
                format!("{}.{}", path, k)
            };
            differences.insert(new_path, (None, Some(v.clone())));
        }
    }

    differences
}

fn compare_json_array(
    a: &Vec<Value>,
    b: &Vec<Value>,
    path: String,
) -> HashMap<String, (Option<Value>, Option<Value>)> {
    let mut differences = HashMap::new();

    let len = a.len().max(b.len());
    for i in 0..len {
        let new_path = format!("{}[{}]", path, i);
        differences.extend(compare_json(
            a.get(i).unwrap_or(&Value::Null),
            b.get(i).unwrap_or(&Value::Null),
            new_path,
        ));
    }

    differences
}

#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub retry_message: String,
}

impl ExponentialBackoff {
    // Keep retrying until the operation returns Some or we reach the max number of retries.
    pub async fn retry<F, Fut, T>(&self, mut operation: F) -> Option<T>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Option<T>>,
    {
        for retry in 1..=self.max_retries {
            if let Some(result) = operation().await {
                return Some(result);
            }
            if retry == self.max_retries {
                break;
            }
            let delay = self.base_delay * retry;
            tracing::warn!(
                "{} Retrying in {} seconds",
                self.retry_message,
                delay.as_secs()
            );
            sleep(delay).await;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_same_json() {
        let json1 = json!({
            "key1": "value1",
            "key2": 2,
            "key3": [
                "value2",
                "+value3"
            ]
        });

        let differences = compare_json(&json1, &json1, "".to_string());
        assert_eq!(differences.len(), 0);
    }

    #[test]
    fn test_deeply_nested_objects() {
        let a = json!({
            "key1": {
                "subkey1": {
                    "subsubkey1": "value1",
                    "subsubkey2": "value2"
                },
                "subkey2": "value3"
            },
            "key2": "value4"
        });

        let b = json!({
            "key1": {
                "subkey1": {
                    "subsubkey1": "value1",
                    "subsubkey2": "value5"
                },
                "subkey2": "value6"
            },
            "key2": "value4"
        });

        let differences = compare_json(&a, &b, "".to_string());

        assert_eq!(differences.len(), 2);
        assert_eq!(
            differences.get("key1.subkey1.subsubkey2"),
            Some(&(Some(json!("value2")), Some(json!("value5"))))
        );
        assert_eq!(
            differences.get("key1.subkey2"),
            Some(&(Some(json!("value3")), Some(json!("value6"))))
        );
    }

    #[test]
    fn test_diff_different_keys() {
        let a = json!({
            "key1": "value1",
            "key2": "value2"
        });

        let b = json!({
            "key1": "value1",
            "key3": "value3"
        });

        let differences = compare_json(&a, &b, "".to_string());

        assert_eq!(differences.len(), 2);
        assert_eq!(
            differences.get("key2"),
            Some(&(Some(json!("value2")), None))
        );
        assert_eq!(
            differences.get("key3"),
            Some(&(None, Some(json!("value3"))))
        );
    }

    #[test]
    fn test_diff_different_types() {
        let a = json!({
            "key1": true,
            "key2": 123,
            "key3": "value1"
        });

        let b = json!({
            "key1": false,
            "key2": "123",
            "key3": "value2"
        });

        let differences = compare_json(&a, &b, "".to_string());

        assert_eq!(differences.len(), 3);
        assert_eq!(
            differences.get("key1"),
            Some(&(Some(json!(true)), Some(json!(false))))
        );
        assert_eq!(
            differences.get("key2"),
            Some(&(Some(json!(123)), Some(json!("123"))))
        );
        assert_eq!(
            differences.get("key3"),
            Some(&(Some(json!("value1")), Some(json!("value2"))))
        );
    }

    #[test]
    fn test_empty_jsons() {
        let json1 = json!({});
        let json2 = json!([]);

        let differences = compare_json(&json1, &json1, "".to_string());
        assert_eq!(differences.len(), 0);

        let differences = compare_json(&json2, &json2, "".to_string());
        assert_eq!(differences.len(), 0);

        let differences = compare_json(&json1, &json2, "".to_string());
        assert_eq!(differences.len(), 1);
    }

    #[test]
    fn test_one_empty_json() {
        let json1 = json!({});
        let json2 = json!({
            "key1": "value1",
            "key2": 2,
        });

        let differences = compare_json(&json1, &json2, "".to_string());
        assert_eq!(differences.len(), 2);

        let differences = compare_json(&json2, &json1, "".to_string());
        assert_eq!(differences.len(), 2);
    }

    #[test]
    fn test_json_with_null() {
        let a = json!({
            "key1": null,
            "key2": "value2"
        });

        let b = json!({
            "key1": "value1",
            "key2": null
        });

        let differences = compare_json(&a, &b, "".to_string());

        assert_eq!(differences.len(), 2);
        assert_eq!(
            differences.get("key1"),
            Some(&(None, Some(json!("value1"))))
        );
        assert_eq!(
            differences.get("key2"),
            Some(&(Some(json!("value2")), None))
        );
    }

    #[test]
    fn test_arrays_different_lengths() {
        let a = json!([1, 2, 3]);
        let b = json!([1, 2, 3, 4]);

        let differences = compare_json(&a, &b, "".to_string());

        assert_eq!(differences.len(), 1);
        assert_eq!(differences.get("[3]"), Some(&(None, Some(json!(4)))));
    }

    #[test]
    fn test_arrays_with_nested_objects() {
        let a = json!([{"key1": "value1"}, {"key2": "value2"}]);
        let b = json!([{"key1": "value1"}, {"key2": "value3"}]);

        let differences = compare_json(&a, &b, "".to_string());

        assert_eq!(differences.len(), 1);
        assert_eq!(
            differences.get("[1].key2"),
            Some(&(Some(json!("value2")), Some(json!("value3"))))
        );
    }
}
