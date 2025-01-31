use tracing::{span, Subscriber};
use tracing_subscriber::{fmt, registry::LookupSpan, Layer};

/// Implementation of statically typed logs layer, which can be either plain or JSON.
/// This is mostly required to avoid [boxing the layer][layer_box].
///
/// [layer_box]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/trait.Layer.html#method.boxed
#[derive(Debug)]
pub enum LogsLayer<S> {
    Plain(fmt::Layer<S>),
    Json(JsonLayer<S>),
}

macro_rules! dispatch_layer {
    ($self:ident.$method:ident($($arg:ident),*)) => {
        match $self {
            LogsLayer::Plain(layer) => layer.$method($($arg),*),
            LogsLayer::Json(layer) => layer.$method($($arg),*),
        }
    };
}

// Implementation note: methods like `and_then`, `with_filter`, `with_timer`, etc. are not
// implemented because they wrap `Self`, so default implementation is sufficient.
impl<S> tracing_subscriber::Layer<S> for LogsLayer<S>
where
    S: Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
{
    fn on_register_dispatch(&self, collector: &tracing::Dispatch) {
        dispatch_layer!(self.on_register_dispatch(collector));
    }

    fn on_layer(&mut self, subscriber: &mut S) {
        dispatch_layer!(self.on_layer(subscriber));
    }

    fn register_callsite(
        &self,
        metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        dispatch_layer!(self.register_callsite(metadata))
    }

    fn enabled(
        &self,
        metadata: &tracing::Metadata<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        dispatch_layer!(self.enabled(metadata, ctx))
    }

    fn on_new_span(
        &self,
        attrs: &span::Attributes<'_>,
        id: &span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        dispatch_layer!(self.on_new_span(attrs, id, ctx))
    }

    fn on_record(
        &self,
        span: &span::Id,
        values: &span::Record<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        dispatch_layer!(self.on_record(span, values, ctx))
    }

    fn on_follows_from(
        &self,
        span: &span::Id,
        follows: &span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        dispatch_layer!(self.on_follows_from(span, follows, ctx))
    }

    fn event_enabled(
        &self,
        event: &tracing::Event<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        dispatch_layer!(self.event_enabled(event, ctx))
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        dispatch_layer!(self.on_event(event, ctx))
    }

    fn on_enter(&self, id: &span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        dispatch_layer!(self.on_enter(id, ctx))
    }

    fn on_exit(&self, id: &span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        dispatch_layer!(self.on_exit(id, ctx))
    }

    fn on_close(&self, id: span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        dispatch_layer!(self.on_close(id, ctx))
    }

    fn on_id_change(
        &self,
        old: &span::Id,
        new: &span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        dispatch_layer!(self.on_id_change(old, new, ctx))
    }

    fn boxed(self) -> Box<dyn Layer<S> + Send + Sync + 'static>
    where
        Self: Sized,
        Self: Layer<S> + Send + Sync + 'static,
        S: Subscriber,
    {
        dispatch_layer!(self.boxed())
    }
}

// I guess tracing types weren't supposed to be written, but we have to.
// If this type has to be changed, the easiest way to figure it out is to attempt
// constructing the object, e.g.:
// ```
// let layer = fmt::Layer::default()
//     .with_file(true)
//     .with_line_number(true)
//     .with_timer(timer)
//     .json();
// ```
// Compiler will complain and tell the type for you.
type JsonLayer<S> = tracing_subscriber::fmt::Layer<
    S,
    tracing_subscriber::fmt::format::JsonFields,
    tracing_subscriber::fmt::format::Format<
        tracing_subscriber::fmt::format::Json,
        tracing_subscriber::fmt::time::UtcTime<time::format_description::well_known::Rfc3339>,
    >,
>;
