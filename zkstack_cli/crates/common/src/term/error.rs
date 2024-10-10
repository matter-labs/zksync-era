use crate::logger;

pub fn log_error(error: anyhow::Error) {
    logger::error(error.to_string());

    if error.chain().count() > 1 {
        logger::warn(
            // "Caused by:",
            error
                .chain()
                .skip(1)
                .enumerate()
                .map(|(i, cause)| format!("  {i}: {}", cause))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    logger::outro("Failed to run command");
}
