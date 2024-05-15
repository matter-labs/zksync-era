use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
};

use serde::Serialize;
use xshell::Shell;

pub fn save_yaml_file(
    shell: &Shell,
    file_path: impl AsRef<Path>,
    content: impl Serialize,
) -> anyhow::Result<()> {
    let data = serde_yaml::to_string(&content)?;
    shell.write_file(file_path, data)?;
    Ok(())
}

pub fn save_toml_file(
    shell: &Shell,
    file_path: impl AsRef<Path>,
    content: impl Serialize,
) -> anyhow::Result<()> {
    let data = toml::to_string(&content)?;
    shell.write_file(file_path, data)?;
    Ok(())
}

pub fn save_json_file(
    shell: &Shell,
    file_path: impl AsRef<Path>,
    content: impl Serialize,
) -> anyhow::Result<()> {
    let data = serde_json::to_string_pretty(&content)?;
    shell.write_file(file_path, data)?;
    Ok(())
}

pub fn prepend_file(file_path: impl AsRef<Path>, data: &[u8]) -> anyhow::Result<()> {
    // Create a temporary file, stopping it from being deleted when dropped
    let (mut tmp, tmp_path) = tempfile::NamedTempFile::new()?.keep()?;
    // Open source file for reading
    let mut src = File::open(&file_path)?;
    // Write the data to prepend
    tmp.write_all(data)?;
    // Copy the rest of the source file
    io::copy(&mut src, &mut tmp)?;
    // Move the temp file to the source file
    fs::remove_file(&file_path)?;
    fs::rename(tmp_path, &file_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_prepends_data_to_file() -> anyhow::Result<()> {
        // Given
        let mut file = tempfile::NamedTempFile::new()?;
        file.write_all(b"Hello, world!")?;

        // When
        prepend_file(file.path(), b"Prepended data.\n").expect("Failed to prepend data to file");

        // Then
        let content = fs::read_to_string(file.path())?;
        assert_eq!(content, "Prepended data.\nHello, world!");

        Ok(())
    }
}
