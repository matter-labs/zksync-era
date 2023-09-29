use crate::synthesized_circuit_provider::SharedAssemblyQueue;
use queues::IsQueue;
use std::net::{IpAddr, SocketAddr};
use std::time::Instant;
use zksync_dal::ConnectionPool;
use zksync_types::proofs::{GpuProverInstanceStatus, SocketAddress};

use tokio::{
    io::copy,
    net::{TcpListener, TcpStream},
};

#[allow(clippy::too_many_arguments)]
pub async fn incoming_socket_listener(
    host: IpAddr,
    port: u16,
    queue: SharedAssemblyQueue,
    pool: ConnectionPool,
    specialized_prover_group_id: u8,
    region: String,
    zone: String,
    num_gpu: u8,
) {
    let listening_address = SocketAddr::new(host, port);
    vlog::info!(
        "Starting assembly receiver at host: {}, port: {}",
        host,
        port
    );
    let listener = TcpListener::bind(listening_address)
        .await
        .unwrap_or_else(|_| panic!("Failed binding address: {:?}", listening_address));
    let address = SocketAddress { host, port };

    let queue_capacity = queue.lock().await.capacity();
    pool.access_storage()
        .await
        .gpu_prover_queue_dal()
        .insert_prover_instance(
            address.clone(),
            queue_capacity,
            specialized_prover_group_id,
            region.clone(),
            zone.clone(),
            num_gpu,
        )
        .await;

    let mut now = Instant::now();

    loop {
        let stream = match listener.accept().await {
            Ok(stream) => stream.0,
            Err(e) => {
                panic!("could not accept connection: {:?}", e);
            }
        };
        vlog::trace!(
            "Received new assembly send connection, waited for {}ms.",
            now.elapsed().as_millis()
        );

        handle_incoming_file(
            stream,
            queue.clone(),
            pool.clone(),
            address.clone(),
            region.clone(),
            zone.clone(),
        )
        .await;

        now = Instant::now();
    }
}

async fn handle_incoming_file(
    mut stream: TcpStream,
    queue: SharedAssemblyQueue,
    pool: ConnectionPool,
    address: SocketAddress,
    region: String,
    zone: String,
) {
    let mut assembly: Vec<u8> = vec![];
    let started_at = Instant::now();
    copy(&mut stream, &mut assembly)
        .await
        .expect("Failed reading from stream");
    let file_size_in_gb = assembly.len() / (1024 * 1024 * 1024);
    vlog::trace!(
        "Read file of size: {}GB from stream took: {} seconds",
        file_size_in_gb,
        started_at.elapsed().as_secs()
    );
    // acquiring lock from queue and updating db must be done atomically otherwise it results in TOCTTOU
    // Time-of-Check to Time-of-Use
    let mut assembly_queue = queue.lock().await;
    let (queue_free_slots, status) = {
        assembly_queue
            .add(assembly)
            .expect("Failed saving assembly to queue");
        let status = if assembly_queue.capacity() == assembly_queue.size() {
            GpuProverInstanceStatus::Full
        } else {
            GpuProverInstanceStatus::Available
        };
        let queue_free_slots = assembly_queue.capacity() - assembly_queue.size();
        (queue_free_slots, status)
    };

    pool.access_storage()
        .await
        .gpu_prover_queue_dal()
        .update_prover_instance_status(address, status, queue_free_slots, region, zone)
        .await;
}
