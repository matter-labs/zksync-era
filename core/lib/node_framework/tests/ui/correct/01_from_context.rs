#![allow(dead_code)]

use zksync_node_framework::{FromContext, Resource};

#[derive(Clone)]
struct ResourceA;

impl Resource for ResourceA {
    fn name() -> String {
        "a".to_string()
    }
}

#[derive(Clone, Default)]
struct ResourceB;

impl Resource for ResourceB {
    fn name() -> String {
        "b".to_string()
    }
}

#[derive(FromContext)]
struct SimpleStruct {
    _field: ResourceA,
    _field_2: ResourceB,
}

#[derive(FromContext)]
struct StructWithDefault {
    _field: ResourceA,
    #[context(default)]
    _field_default: ResourceB,
}

#[derive(FromContext)]
struct StructWithOption {
    _field: Option<ResourceA>,
}

fn main() {}
