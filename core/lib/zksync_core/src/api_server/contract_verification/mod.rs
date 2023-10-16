use std::{net::SocketAddr, time::Duration};

use actix_cors::Cors;
use actix_web::{
    dev::Server,
    {web, App, HttpResponse, HttpServer},
};
use tokio::{sync::watch, task::JoinHandle};

use zksync_config::configs::api::ContractVerificationApiConfig;
use zksync_dal::connection::ConnectionPool;
use zksync_utils::panic_notify::{spawn_panic_handler, ThreadPanicNotify};

mod api_decl;
mod api_impl;
mod metrics;

use self::api_decl::RestApi;

fn start_server(api: RestApi, bind_to: SocketAddr, threads: usize) -> Server {
    HttpServer::new(move || {
        let api = api.clone();
        App::new()
            .wrap(
                Cors::default()
                    .send_wildcard()
                    .max_age(3600)
                    .allow_any_origin()
                    .allow_any_header()
                    .allow_any_method(),
            )
            .service(api.into_scope())
            // Endpoint needed for js isReachable
            .route(
                "/favicon.ico",
                web::get().to(|| async { HttpResponse::Ok().finish() }),
            )
    })
    .workers(threads)
    .bind(bind_to)
    .unwrap()
    .shutdown_timeout(60)
    .keep_alive(Duration::from_secs(10))
    .client_request_timeout(Duration::from_secs(60))
    .disable_signals()
    .run()
}

/// Start HTTP REST API
pub fn start_server_thread_detached(
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    api_config: ContractVerificationApiConfig,
    mut stop_receiver: watch::Receiver<bool>,
) -> JoinHandle<anyhow::Result<()>> {
    let (handler, panic_sender) = spawn_panic_handler();

    std::thread::Builder::new()
        .name("contract-verification-api".to_string())
        .spawn(move || {
            let _panic_sentinel = ThreadPanicNotify(panic_sender.clone());

            actix_rt::System::new().block_on(async move {
                let bind_address = api_config.bind_addr();
                let threads = api_config.threads_per_server as usize;
                let api = RestApi::new(master_connection_pool, replica_connection_pool);

                let server = start_server(api, bind_address, threads);
                let close_handle = server.handle();
                actix_rt::spawn(async move {
                    if stop_receiver.changed().await.is_ok() {
                        close_handle.stop(true).await;
                        tracing::info!(
                            "Stop signal received, contract verification API is shutting down"
                        );
                    }
                });
                server.await.expect("Contract verification API crashed");
            });
        })
        .expect("Failed to spawn thread for REST API");

    handler
}
