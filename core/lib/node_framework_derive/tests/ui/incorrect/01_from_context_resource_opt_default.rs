use zksync_node_framework_derive::FromContext;

mod service {
    pub struct ServiceContext<'a> {
        _marker: std::marker::PhantomData<&'a ()>,
    }

    impl<'a> ServiceContext<'a> {
        pub fn get_resource<T>(&mut self) -> Result<T, crate::WiringError> {
            unimplemented!()
        }

        pub fn get_resource_or_default<T>(&mut self) -> Result<T, crate::WiringError> {
            unimplemented!()
        }
    }
}

pub enum WiringError {
    ResourceLacking { _name: String },
}

pub trait FromContext: Sized {
    fn from_context(context: &mut service::ServiceContext<'_>) -> Result<Self, WiringError>;
}

#[derive(FromContext)]
#[ctx(local)]
struct StructWithOption {
    #[resource(default)]
    _field: Option<u8>,
}

fn main() {}
