use std::{
    io::{copy, ErrorKind, Read},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

pub fn send_assembly(
    job_id: u32,
    mut serialized: &[u8],
    socket_address: &SocketAddr,
) -> Result<(Duration, u64), String> {
    tracing::trace!(
        "Sending assembly to {}:{}, job id {{{job_id}}}",
        socket_address.ip(),
        socket_address.port()
    );

    let started_at = Instant::now();
    let mut error_messages = vec![];

    // 10 attempts, each waiting for 6 seconds => 1 minute wait time at most
    for _ in 0..10 {
        match TcpStream::connect_timeout(socket_address, Duration::from_secs(6)) {
            Ok(mut stream) => {
                return send(&mut serialized, &mut stream)
                    .map(|result| (started_at.elapsed(), result))
                    .map_err(|err| format!("Could not send assembly to prover: {err:?}"));
            }
            Err(err) => {
                error_messages.push(format!("{err:?}"));
            }
        }
    }

    Err(format!(
        "Could not establish connection with prover after several attempts: {error_messages:?}"
    ))
}

fn send(read: &mut impl Read, tcp: &mut TcpStream) -> std::io::Result<u64> {
    let mut attempts = 10;
    let mut last_result = Ok(0);

    while attempts > 0 {
        match copy(read, tcp) {
            Ok(copied) => return Ok(copied),
            Err(err) if can_be_retried(err.kind()) => {
                attempts -= 1;
                last_result = Err(err);
            }
            Err(err) => return Err(err),
        }

        std::thread::sleep(Duration::from_millis(50));
    }

    last_result
}

fn can_be_retried(err: ErrorKind) -> bool {
    matches!(err, ErrorKind::TimedOut | ErrorKind::ConnectionRefused)
}
