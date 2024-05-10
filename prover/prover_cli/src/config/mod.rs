pub fn get_envfile() -> anyhow::Result<String> {
    if let Ok(envfile) = std::env::var("PLI__CONFIG") {
        return Ok(envfile);
    }
    Ok(std::env::var("ZKSYNC_HOME").map(|home| home + "/etc/pliconfig")?)
}

pub fn load_envfile(path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
    std::fs::read_to_string(path)?
        .lines()
        .filter(|l| !l.starts_with("#"))
        .filter_map(|l| l.split_once("="))
        .for_each(|(k, v)| std::env::set_var(k, v));

    Ok(())
}
