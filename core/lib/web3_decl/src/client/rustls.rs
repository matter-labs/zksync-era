/// Makes sure that `rustls` crypto backend is set before we instantiate
/// a `Web3` client. `jsonrpsee` doesn't explicitly set it, and when
/// multiple crypto backends are enabled, `rustls` can't choose one and panics.
/// See [this issue](https://github.com/rustls/rustls/issues/1877) for more detail.
///
/// The problem is on `jsonrpsee` side, but until it's fixed we have to patch it.
pub(super) fn set_rustls_backend_if_required() {
    // Function returns an error if the provider is already installed, and we're fine with it.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}
