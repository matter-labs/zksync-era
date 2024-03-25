fn main() {
    let database_url = std::env::var("DATABASE_PROVER_URL");
    match database_url {
        Ok(url) => {
            println!("cargo:rustc-env=DATABASE_URL={}", url);
        }
        Err(_) => {
            println!("cargo:warning=DATABASE_PROVER_URL is not set");
        }
    }
}
