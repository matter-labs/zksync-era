use std::io::Write;
use zksync_dal::ConnectionPool;
use zksync_types::explorer_api::SourceCodeData;

fn main() {
    let pool = ConnectionPool::new(Some(1), false);
    let mut storage = pool.access_storage_blocking();
    let reqs = storage
        .explorer()
        .contract_verification_dal()
        .get_all_successful_requests()
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
                let mut file =
                    std::fs::File::create(format!("{}/{}.sol", &dir, req.req.contract_name))
                        .unwrap();
                file.write_all(content.as_bytes()).unwrap();
            }
            SourceCodeData::YulSingleFile(content) => {
                let mut file =
                    std::fs::File::create(format!("{}/{}.yul", &dir, req.req.contract_name))
                        .unwrap();
                file.write_all(content.as_bytes()).unwrap();
            }
            SourceCodeData::StandardJsonInput(input) => {
                let sources = input.get(&"sources".to_string()).unwrap().clone();
                for (key, val) in sources.as_object().unwrap() {
                    let p = format!("{}/{}", &dir, key);
                    let path = std::path::Path::new(p.as_str());
                    let prefix = path.parent().unwrap();
                    std::fs::create_dir_all(prefix).unwrap();
                    let mut file = std::fs::File::create(path).unwrap();
                    let content = val.get(&"content".to_string()).unwrap().as_str().unwrap();
                    file.write_all(content.as_bytes()).unwrap();
                }
            }
        }
    }
}
