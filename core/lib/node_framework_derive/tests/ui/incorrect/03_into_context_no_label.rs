#![allow(dead_code)]

use zksync_node_framework_derive::IntoContext;

mod service {
    pub struct ServiceContext<'a> {
        _marker: std::marker::PhantomData<&'a ()>,
    }

    impl<'a> ServiceContext<'a> {
        pub fn add_task<T>(&mut self, _task: T) -> &mut Self {
            unimplemented!()
        }

        pub fn insert_resource<T>(&mut self, _resource: T) -> Result<(), super::WiringError> {
            unimplemented!()
        }
    }
}

pub enum WiringError {
    ResourceLacking { _name: String },
}

pub trait IntoContext {
    fn into_context(self, context: &mut service::ServiceContext<'_>) -> Result<(), WiringError>;
}

#[derive(IntoContext)]
#[ctx(local)]
struct SimpleStruct {
    #[resource]
    _field: u8,
    _field_2: u16,
}

fn main() {}
