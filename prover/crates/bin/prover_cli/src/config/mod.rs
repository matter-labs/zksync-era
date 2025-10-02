use std::{io::Write, path::PathBuf};

use zksync_utils::env::Workspace;

pub fn get_envfile() -> anyhow::Result<PathBuf> {
    if let Ok(envfile) = std::env::var("PLI__CONFIG") {
        return Ok(envfile.into());
    }
    Ok(Workspace::locate().root().join("etc/pliconfig"))
}

pub fn load_envfile(path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
    std::fs::read_to_string(path)?
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .for_each(|(k, v)| std::env::set_var(k, v));
    Ok(())
}

pub fn update_envfile(
    path: impl AsRef<std::path::Path> + std::marker::Copy,
    key: impl AsRef<str>,
    value: impl AsRef<str>,
) -> anyhow::Result<()> {
    let prefix = format!("{}=", key.as_ref());
    let kv = format!("{}={}", key.as_ref(), value.as_ref());
    let swapfile = path.as_ref().with_extension(".swp");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&swapfile)?);
    let mut found = false;

    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| {
            if l.starts_with(&prefix) {
                found = true;
                kv.clone()
            } else {
                l.to_string()
            }
        })
        .try_for_each(|l| writeln!(&mut out, "{}", l))?;
    if !found {
        writeln!(&mut out, "{}", kv)?;
    }
    out.flush()?;
    std::fs::rename(swapfile, path)?;

    Ok(())
}
