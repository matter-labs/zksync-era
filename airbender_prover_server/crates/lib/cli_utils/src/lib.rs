// TODO: This file is messy, to be refactored

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use zksync_airbender_verifier::types::AirbenderVerifierInput;

/// Initialize the global tracing subscriber for a binary.
///
/// Log verbosity follows the standard `RUST_LOG` environment variable (via
/// `EnvFilter`), defaulting to `info`. The output format is selected by the
/// `LOG_FORMAT` environment variable:
///
/// * `LOG_FORMAT=json` — newline-delimited structured JSON, suitable for log
///   aggregation pipelines (Loki, Datadog, ...).
/// * anything else or unset — the default human-readable text format.
///
/// When the crate's `sentry` feature is enabled (the server binary turns it on),
/// a Sentry layer is also attached so `tracing` errors become Sentry events and
/// lower-level spans become breadcrumbs. The layer is inert until a Sentry
/// client is initialized, so enabling the feature alone has no effect.
///
/// Centralizing this keeps every binary's logging behavior identical.
pub fn init_tracing() -> Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, Layer};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let json = std::env::var("LOG_FORMAT")
        .map(|value| value.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    // Box the format layer so the JSON and text variants share one type.
    let fmt_layer = tracing_subscriber::fmt::layer().with_target(false);
    let fmt_layer = if json {
        fmt_layer.json().boxed()
    } else {
        fmt_layer.boxed()
    };

    let subscriber = tracing_subscriber::registry().with(filter).with(fmt_layer);
    #[cfg(feature = "sentry")]
    let subscriber = subscriber.with(sentry::integrations::tracing::layer());

    // `try_init` returns a `TryInitError` that does not implement
    // `std::error::Error`, so `.context()` is unavailable here.
    subscriber
        .try_init()
        .map_err(|err| anyhow::anyhow!("while attempting to initialize tracing subscriber: {err}"))
}

/// Shared representation of a repository-owned batch input file.
///
/// We keep both the logical batch number and the exact file path so CLI tools can
/// select inputs once and then process concrete files without repeating path
/// probing logic.
#[derive(Debug, Clone)]
pub struct BatchInputFile {
    pub number: u64,
    pub path: PathBuf,
}

/// Resolve the batch files requested by a CLI into concrete filesystem paths.
///
/// The repository stores one input per file, and the CLI now accepts that
/// concrete filename directly. Centralizing the resolution logic keeps the
/// different binaries aligned on supported extensions, preserves the caller's
/// explicit ordering for curated lists, and keeps the Git LFS friendly error
/// messages in one place.
pub fn resolve_batch_inputs(
    batches_dir: &Path,
    batch_files: Option<&[PathBuf]>,
    all_batches: bool,
) -> Result<Vec<BatchInputFile>> {
    if all_batches {
        return list_all_batch_inputs(batches_dir);
    }

    let batch_files = batch_files.context(
        "while attempting to select input batches, pass either --batch-files <a.bin[.gz],b.bin[.gz]> or --all-batches",
    )?;

    batch_files
        .iter()
        .map(|batch_file| resolve_batch_input(batches_dir, batch_file))
        .collect()
}

/// Load and deserialize a `AirbenderVerifierInput` from a batch file.
///
/// Inputs are stored as hex-encoded framed bytes — first 4 bytes (big-endian
/// `u32`) are the bincode payload length, followed by the payload itself
/// padded out to a multiple of 4 bytes. The files may live as plain `.bin`
/// or as gzipped `.bin.gz` tracked in Git LFS; the format detail is hidden
/// from callers.
///
/// Returns the versioned wire wrapper; callers extract the payload with
/// `.into_v1()`.
pub fn load_batch(batch_input: &BatchInputFile) -> Result<AirbenderVerifierInput> {
    let raw = read_batch_text(&batch_input.path)
        .with_context(|| format!("while attempting to read {}", batch_input.path.display()))?;
    let mut bytes = parse_hex_bytes(&raw).with_context(|| {
        format!(
            "while attempting to parse hex bytes for batch {} from {}",
            batch_input.number,
            batch_input.path.display()
        )
    })?;
    anyhow::ensure!(
        bytes.len() >= 4,
        "batch {}: framed payload too short ({} bytes)",
        batch_input.number,
        bytes.len()
    );
    let byte_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    bytes.drain(..4);
    anyhow::ensure!(
        bytes.len() >= byte_len,
        "batch {}: declared length {byte_len} exceeds available bytes {}",
        batch_input.number,
        bytes.len()
    );
    bytes.truncate(byte_len);

    let (input, decoded_len): (AirbenderVerifierInput, usize) =
        bincode::serde::decode_from_slice(&bytes, bincode::config::standard())
            .with_context(|| format!("while decoding batch {} as bincode", batch_input.number))?;
    anyhow::ensure!(
        decoded_len == bytes.len(),
        "batch {}: trailing bytes after bincode decode ({decoded_len} of {})",
        batch_input.number,
        bytes.len(),
    );
    Ok(input)
}

fn list_all_batch_inputs(batches_dir: &Path) -> Result<Vec<BatchInputFile>> {
    let entries = std::fs::read_dir(batches_dir)
        .with_context(|| format!("while attempting to read {}", batches_dir.display()))?;

    let mut batch_inputs = BTreeMap::<u64, PathBuf>::new();
    for entry in entries {
        let entry = entry.context("while attempting to read directory entry")?;
        let path = entry.path();
        match parse_batch_number_from_path(&path) {
            Some(number) => {
                if let Some(previous_path) = batch_inputs.insert(number, path.clone()) {
                    anyhow::bail!(
                        "found multiple files for batch {number}: {} and {}",
                        previous_path.display(),
                        path.display()
                    );
                }
            }
            None if is_supported_batch_path(&path) => {
                tracing::warn!(path = %path.display(), "Skipping batch file with unsupported name");
            }
            None => {}
        }
    }

    if batch_inputs.is_empty() {
        anyhow::bail!("no batch files were found in {}", batches_dir.display());
    }

    Ok(batch_inputs
        .into_iter()
        .map(|(number, path)| BatchInputFile { number, path })
        .collect())
}

fn resolve_batch_input(batches_dir: &Path, batch_file: &Path) -> Result<BatchInputFile> {
    // We accept the CLI argument as a concrete filename so callers do not need
    // to understand our fallback rules. Relative paths resolve within the batch
    // corpus directory; absolute paths are respected as-is for local experiments.
    let batch_path = if batch_file.is_absolute() {
        batch_file.to_path_buf()
    } else {
        batches_dir.join(batch_file)
    };

    if !batch_path.is_file() {
        anyhow::bail!(
            "batch input {} does not exist; expected a file named <number>.bin or <number>.bin.gz",
            batch_path.display()
        );
    }

    let number = parse_batch_number_from_path(&batch_path).with_context(|| {
        format!(
            "while attempting to parse batch number from {}, expected <number>.bin or <number>.bin.gz",
            batch_path.display()
        )
    })?;

    Ok(BatchInputFile {
        number,
        path: batch_path,
    })
}

fn parse_batch_number_from_path(path: &Path) -> Option<u64> {
    if path.extension().and_then(|ext| ext.to_str()) == Some("bin") {
        return path
            .file_stem()
            .and_then(|stem| stem.to_str())?
            .parse()
            .ok();
    }

    if path.extension().and_then(|ext| ext.to_str()) == Some("gz") {
        let compressed_stem = path.file_stem().and_then(|stem| stem.to_str())?;
        let nested = Path::new(compressed_stem);
        if nested.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            return None;
        }
        return nested
            .file_stem()
            .and_then(|stem| stem.to_str())?
            .parse()
            .ok();
    }

    None
}

fn is_supported_batch_path(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("bin")
        || path.extension().and_then(|ext| ext.to_str()) == Some("gz")
}

fn read_batch_text(batch_path: &Path) -> Result<String> {
    match batch_path.extension().and_then(|ext| ext.to_str()) {
        Some("bin") => std::fs::read_to_string(batch_path)
            .with_context(|| format!("while attempting to read {}", batch_path.display())),
        Some("gz") => read_gzip_batch_text(batch_path),
        _ => anyhow::bail!(
            "unsupported batch input path {}, expected a .bin or .bin.gz file",
            batch_path.display()
        ),
    }
}

fn read_gzip_batch_text(batch_path: &Path) -> Result<String> {
    let compressed_bytes = std::fs::read(batch_path)
        .with_context(|| format!("while attempting to read {}", batch_path.display()))?;

    const GIT_LFS_POINTER_PREFIX: &str = "version https://git-lfs.github.com/spec/v1";
    if compressed_bytes.starts_with(GIT_LFS_POINTER_PREFIX.as_bytes()) {
        anyhow::bail!(
            "{} is still a Git LFS pointer; pull the matching LFS object before retrying",
            batch_path.display()
        );
    }

    let mut decoder = GzDecoder::new(compressed_bytes.as_slice());
    let mut raw = String::new();
    decoder.read_to_string(&mut raw).with_context(|| {
        format!(
            "while attempting to decompress UTF-8 text from {}",
            batch_path.display()
        )
    })?;
    Ok(raw)
}

fn parse_hex_bytes(raw: &str) -> Result<Vec<u8>> {
    let mut compact: String = raw.chars().filter(|ch| !ch.is_whitespace()).collect();
    if let Some(stripped) = compact.strip_prefix("0x") {
        compact = stripped.to_owned();
    }

    if compact.is_empty() {
        anyhow::bail!("batch payload is empty");
    }
    // Files are stored in 8-hex-char (= 4-byte) words; require alignment so we
    // catch truncated files early.
    if !compact.len().is_multiple_of(8) {
        anyhow::bail!(
            "batch payload length must be a multiple of 8 hex characters (got {})",
            compact.len()
        );
    }

    let mut bytes = Vec::with_capacity(compact.len() / 2);
    for chunk in compact.as_bytes().chunks(2) {
        let s = std::str::from_utf8(chunk).context("while decoding hex chunk as UTF-8")?;
        let byte =
            u8::from_str_radix(s, 16).with_context(|| format!("while parsing hex byte `{s}`"))?;
        bytes.push(byte);
    }
    Ok(bytes)
}
