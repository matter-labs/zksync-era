use std::io::Write;

use zksync_config::{configs::DatabaseSecrets, full_config_schema, sources::ConfigFilePaths};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::contract_verification::api::SourceCodeData;

#[tokio::main]
async fn main() {
    let config_sources = ConfigFilePaths::default()
        .into_config_sources("ZKSYNC_")
        .unwrap();

    let schema = full_config_schema();
    let repo = config_sources.build_repository(&schema);
    let config: DatabaseSecrets = repo.single().unwrap().parse().unwrap();

    let pool = ConnectionPool::<Core>::singleton(config.replica_url().unwrap())
        .build()
        .await
        .unwrap();
    let mut storage = pool.connection().await.unwrap();
    let reqs = storage
        .contract_verification_dal()
        .get_all_successful_requests()
        .await
        .unwrap();

    std::fs::create_dir_all("./verified_sources").unwrap();
    for req in reqs {
        let dir = format!("./verified_sources/{:?}", req.req.contract_address);
        if std::path::Path::new(&dir).exists() {
            continue;
        }

        std::fs::create_dir_all(&dir).unwrap();
        let mut file = std::fs::File::create(format!("{}/request.json", &dir)).unwrap();
        file.write_all(serde_json::to_string_pretty(&req.req).unwrap().as_bytes())
            .unwrap();

        match req.req.source_code_data {
            SourceCodeData::SolSingleFile(content) => {
                let contact_name = if let Some((_file_name, contract_name)) =
                    req.req.contract_name.rsplit_once(':')
                {
                    contract_name.to_string()
                } else {
                    req.req.contract_name.clone()
                };

                let mut file =
                    std::fs::File::create(format!("{}/{}.sol", &dir, contact_name)).unwrap();
                file.write_all(content.as_bytes()).unwrap();
            }
            SourceCodeData::YulSingleFile(content) => {
                let mut file =
                    std::fs::File::create(format!("{}/{}.yul", &dir, req.req.contract_name))
                        .unwrap();
                file.write_all(content.as_bytes()).unwrap();
            }
            SourceCodeData::StandardJsonInput(input) => {
                let sources = input.get("sources").unwrap().clone();
                for (key, val) in sources.as_object().unwrap() {
                    let p = format!("{}/{}", &dir, key);
                    let path = std::path::Path::new(p.as_str());
                    let prefix = path.parent().unwrap();
                    std::fs::create_dir_all(prefix).unwrap();
                    let mut file = std::fs::File::create(path).unwrap();
                    let content = val.get("content").unwrap().as_str().unwrap();
                    file.write_all(content.as_bytes()).unwrap();
                }
            }
            SourceCodeData::VyperMultiFile(sources) => {
                for (key, content) in sources {
                    let p = format!("{}/{}.vy", &dir, key);
                    let path = std::path::Path::new(p.as_str());
                    let prefix = path.parent().unwrap();
                    std::fs::create_dir_all(prefix).unwrap();
                    let mut file = std::fs::File::create(path).unwrap();
                    file.write_all(content.as_bytes()).unwrap();
                }
            }
        }
    }
}
