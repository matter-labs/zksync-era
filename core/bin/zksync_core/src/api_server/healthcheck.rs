use actix_web::dev::Server;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use serde::Serialize;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::watch;
use zksync_health_check::{CheckHealth, CheckHealthStatus};
use zksync_utils::panic_notify::{spawn_panic_handler, ThreadPanicNotify};

#[derive(Serialize)]
pub struct Response {
    pub message: String,
}

#[get("/health")]
async fn healthcheck(healthchecks: web::Data<[Box<dyn CheckHealth>]>) -> impl Responder {
    for healthcheck in healthchecks.iter() {
        match healthcheck.check_health().await {
            CheckHealthStatus::NotReady(message) => {
                let response = Response { message };
                return HttpResponse::ServiceUnavailable().json(response);
            }
            CheckHealthStatus::Ready => (),
        }
    }
    let response = Response {
        message: "Everything is working fine".to_string(),
    };
    HttpResponse::Ok().json(response)
}

fn run_server(bind_address: SocketAddr, healthchecks: Vec<Box<dyn CheckHealth>>) -> Server {
    let healthchecks: Arc<[Box<dyn CheckHealth>]> = healthchecks.into();
    let data = web::Data::from(healthchecks);
    HttpServer::new(move || App::new().service(healthcheck).app_data(data.clone()))
        .workers(1)
        .bind(bind_address)
        .unwrap()
        .run()
}

pub struct HealthCheckHandle {
    server: tokio::task::JoinHandle<()>,
    stop_sender: watch::Sender<bool>,
}

impl HealthCheckHandle {
    pub async fn stop(self) {
        self.stop_sender.send(true).ok();
        self.server.await.unwrap();
    }
}

/// Start HTTP healthcheck API
pub fn start_server_thread_detached(
    addr: SocketAddr,
    healthchecks: Vec<Box<dyn CheckHealth>>,
) -> HealthCheckHandle {
    let (handler, panic_sender) = spawn_panic_handler();
    let (stop_sender, mut stop_receiver) = watch::channel(false);
    std::thread::Builder::new()
        .name("healthcheck".to_string())
        .spawn(move || {
            let _panic_sentinel = ThreadPanicNotify(panic_sender.clone());

            actix_rt::System::new().block_on(async move {
                let server = run_server(addr, healthchecks);
                let close_handle = server.handle();
                actix_rt::spawn(async move {
                    if stop_receiver.changed().await.is_ok() {
                        close_handle.stop(true).await;
                        vlog::info!("Stop signal received, Health api is shutting down");
                    }
                });
                server.await.expect("Health api crashed");
            });
        })
        .expect("Failed to spawn thread for REST API");

    HealthCheckHandle {
        server: handler,
        stop_sender,
    }
}
