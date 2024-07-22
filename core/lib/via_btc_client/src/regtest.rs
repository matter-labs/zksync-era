use std::{env, fs, path::PathBuf, process::Command, thread, time::Duration};

use bitcoin::{address::NetworkUnchecked, Address, Network};
use bitcoincore_rpc::{Auth, Client};
use rand::Rng;
use tempfile::TempDir;

use crate::client::BitcoinRpcClient;

const COMPOSE_TEMPLATE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/docker-compose-btc-template.yml"
);

#[allow(unused)]
pub(crate) struct BitcoinRegtest {
    temp_dir: TempDir,
    compose_file: PathBuf,
    rpc_port: u16,
}

impl BitcoinRegtest {
    pub fn new() -> std::io::Result<Self> {
        let temp_dir = TempDir::new()?;
        let rpc_port = rand::thread_rng().gen_range(49152..65535);
        let compose_file = temp_dir
            .path()
            .join(format!("docker-compose-{}.yml", rpc_port));
        Ok(Self {
            temp_dir,
            compose_file,
            rpc_port,
        })
    }

    fn generate_compose_file(&self) -> std::io::Result<()> {
        let template = fs::read_to_string(COMPOSE_TEMPLATE_PATH)?;
        let compose_content = template.replace("{RPC_PORT}", &self.rpc_port.to_string());
        fs::write(&self.compose_file, compose_content)
    }

    fn run(&self) -> std::io::Result<()> {
        self.generate_compose_file()?;

        Command::new("docker")
            .args([
                "compose",
                "-f",
                self.compose_file.to_str().unwrap(),
                "up",
                "-d",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;

        thread::sleep(Duration::from_secs(10));

        Ok(())
    }

    fn stop(&self) -> std::io::Result<()> {
        Command::new("docker")
            .args([
                "compose",
                "-f",
                self.compose_file.to_str().unwrap(),
                "down",
                "--volumes",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;

        Ok(())
    }

    pub(crate) fn get_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.rpc_port)
    }
}

impl Drop for BitcoinRegtest {
    fn drop(&mut self) {
        if let Err(e) = self.stop() {
            eprintln!("Failed to stop Bitcoin regtest: {}", e);
        }
    }
}

pub(crate) struct TestContext {
    pub(crate) _regtest: BitcoinRegtest,
    pub(crate) client: BitcoinRpcClient,
    pub(crate) test_address: Address,
}

impl TestContext {
    pub(crate) async fn setup(address: Option<&str>) -> Self {
        let regtest = BitcoinRegtest::new().expect("Failed to create BitcoinRegtest");
        regtest.run().expect("Failed to run Bitcoin regtest");

        let url = regtest.get_url();
        let client = BitcoinRpcClient::new(&url, "rpcuser", "rpcpassword")
            .expect("Failed to create BitcoinRpcClient");

        // some random address
        let test_address = address
            .unwrap_or("bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080")
            .parse::<Address<NetworkUnchecked>>()
            .unwrap()
            .require_network(Network::Regtest)
            .unwrap();

        Self {
            _regtest: regtest,
            client,
            test_address,
        }
    }
}

#[cfg(test)]
mod tests {
    use bitcoincore_rpc::RpcApi;

    use super::*;

    #[test]
    fn test_bitcoin_regtest() {
        let regtest = BitcoinRegtest::new().expect("Failed to create BitcoinRegtest");
        regtest.run().expect("Failed to run Bitcoin regtest");

        let url = regtest.get_url();
        let rpc = Client::new(
            &url,
            Auth::UserPass("rpcuser".to_string(), "rpcpassword".to_string()),
        )
        .expect("Failed to create RPC client");

        let balance = rpc.get_balance(None, None).expect("Failed to get balance");
        assert!(balance.to_btc() > 0.0);

        let block_count = rpc.get_block_count().expect("Failed to get block count");
        assert!(block_count > 100);

        let wallet_info = rpc.get_wallet_info().expect("Failed to get wallet info");
        assert_eq!(wallet_info.wallet_name, "Alice");
        assert_eq!(wallet_info.wallet_version, 169900);
    }
}
