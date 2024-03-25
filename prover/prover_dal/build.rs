fn main() {
    let database_url =
        std::env::var("DATABASE_PROVER_URL").expect("Failed to load PROVER_DATABASE_URL");
    println!("cargo:rustc-env=DATABASE_URL={}", database_url)
}
