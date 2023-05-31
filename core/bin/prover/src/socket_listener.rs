use crate::synthesized_circuit_provider::SharedAssemblyQueue;
use queues::IsQueue;
use std::io::copy;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::time::Instant;
use zksync_dal::gpu_prover_queue_dal::{GpuProverInstanceStatus, SocketAddress};
use zksync_dal::ConnectionPool;

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
        .unwrap_or_else(|_| panic!("Failed binding address: {:?}", listening_address));
    let address = SocketAddress { host, port };

    pool.access_storage_blocking()
        .gpu_prover_queue_dal()
        .insert_prover_instance(
            address.clone(),
            queue.lock().unwrap().capacity(),
            specialized_prover_group_id,
            region.clone(),
            zone.clone(),
            num_gpu,
        );

    let mut now = Instant::now();

    for stream in listener.incoming() {
        vlog::trace!(
            "Received new assembly send connection, waited for {}ms.",
            now.elapsed().as_millis()
        );

        let stream = stream.expect("Stream closed early");
        handle_incoming_file(
            stream,
            queue.clone(),
            pool.clone(),
            address.clone(),
            region.clone(),
            zone.clone(),
        );

        now = Instant::now();
    }
}

fn handle_incoming_file(
    mut stream: TcpStream,
    queue: SharedAssemblyQueue,
    pool: ConnectionPool,
    address: SocketAddress,
    region: String,
    zone: String,
) {
    let mut assembly: Vec<u8> = vec![];
    let started_at = Instant::now();
    copy(&mut stream, &mut assembly).expect("Failed reading from stream");
    let file_size_in_gb = assembly.len() / (1024 * 1024 * 1024);
    vlog::trace!(
        "Read file of size: {}GB from stream took: {} seconds",
        file_size_in_gb,
        started_at.elapsed().as_secs()
    );
    let mut assembly_queue = queue.lock().unwrap();

    assembly_queue
        .add(assembly)
        .expect("Failed saving assembly to queue");
    let status = if assembly_queue.capacity() == assembly_queue.size() {
        GpuProverInstanceStatus::Full
    } else {
        GpuProverInstanceStatus::Available
    };

    pool.access_storage_blocking()
        .gpu_prover_queue_dal()
        .update_prover_instance_status(
            address,
            status,
            assembly_queue.capacity() - assembly_queue.size(),
            region,
            zone,
        );
}
