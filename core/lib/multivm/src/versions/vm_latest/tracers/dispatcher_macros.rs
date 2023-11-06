#[macro_export]
macro_rules! dispatch_tracers {
    (required:[$( $required:ident ),*],
     optional:[$( $optional:ident ),*],
     $self:ident.$function:ident $params:tt
    ) => {
        $(
            dispatch_tracers!(@call req $required $self.$function $params);
        )*
        $(
            dispatch_tracers!(@call opt $optional $self.$function $params);
        )*
    };

    (@call req $required:ident $self:ident.$function:ident($( $params:expr ),*)) => {
       $self.$required.$function($( $params ),*)
    };

    (@call opt $optional:ident $self:ident.$function:ident($( $params:expr ),*)) => {
        if let Some(tracer) = &mut $self.$optional {
            tracer.$function($( $params ),*)
        }
    };
}
