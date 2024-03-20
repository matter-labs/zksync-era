use validium_mode_example::{helpers, scenario};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    //scenario::run(1, 1, helpers::TxKind::Deploy).await;
    scenario::basic().await;
}
