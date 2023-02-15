use bincode::serialize_into;
use std::fs::File;
use std::io::BufWriter;
use structopt::StructOpt;
use zksync_verification_key_server::get_vk_for_circuit_type;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "json existing json VK's to binary vk",
    about = "converter tool"
)]
struct Opt {
    /// Binary output path of verification keys.
    #[structopt(short)]
    output_bin_path: String,
}

fn main() {
    let opt = Opt::from_args();
    println!("Converting existing json keys to binary");
    generate_bin_vks(opt.output_bin_path);
}

fn generate_bin_vks(output_path: String) {
    for circuit_type in 1..=18 {
        let filename = format!("{}/verification_{}.key", output_path, circuit_type);
        let vk = get_vk_for_circuit_type(circuit_type);
        let mut f = BufWriter::new(File::create(filename).unwrap());
        serialize_into(&mut f, &vk).unwrap();
    }
}
