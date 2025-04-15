use std::{fs::create_dir_all, io::Cursor, path::Path, time::Duration};

#[tracing::instrument(skip_all)]
fn download_initial_setup(key_download_url: &str) -> reqwest::Result<Vec<u8>> {
    tracing::info!("Downloading initial setup from {:?}", key_download_url);

    const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
    let client = reqwest::blocking::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .unwrap();

    const DOWNLOAD_RETRIES: usize = 5;
    let mut retry_count = 0;

    while retry_count < DOWNLOAD_RETRIES {
        let bytes = client
            .get(key_download_url)
            .send()
            .and_then(|response| response.bytes().map(|bytes| bytes.to_vec()));
        match bytes {
            Ok(bytes) => return Ok(bytes),
            Err(e) => {
                tracing::warn!("Failed to download keys: {}", e);
                retry_count += 1
            }
        }

        tracing::warn!("Failed to download keys. Backing off for 5 second");
        std::thread::sleep(Duration::from_secs(5));
    }

    client
        .get(key_download_url)
        .send()
        .and_then(|response| response.bytes().map(|bytes| bytes.to_vec()))
}

#[tracing::instrument(skip_all)]
pub fn download_initial_setup_keys_if_not_present(
    initial_setup_key_path: &Path,
    key_download_url: &str,
) {
    if initial_setup_key_path.exists() {
        tracing::info!(
            "Initial setup already present at {:?}",
            initial_setup_key_path
        );
        return;
    }

    let bytes = download_initial_setup(key_download_url).expect("Failed downloading initial setup");
    let initial_setup_key_dir = initial_setup_key_path.parent().unwrap();
    create_dir_all(initial_setup_key_dir).unwrap_or_else(|_| {
        panic!(
            "Failed creating dirs recursively: {:?}",
            initial_setup_key_dir
        )
    });
    let mut file = std::fs::File::create(initial_setup_key_path)
        .expect("Cannot create file for the initial setup");
    let mut content = Cursor::new(bytes);
    std::io::copy(&mut content, &mut file).expect("Cannot write the downloaded key to the file");
}
