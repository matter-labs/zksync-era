use rand::{distributions::Distribution, Rng};
use zksync_consensus_utils::EncodeDist;
use zksync_protobuf::testonly::{test_encode_all_formats, FmtConv};

use super::SetAttesterCommitteeFile;

impl Distribution<SetAttesterCommitteeFile> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> SetAttesterCommitteeFile {
        SetAttesterCommitteeFile {
            attesters: rng.gen(),
        }
    }
}

#[test]
fn test_encoding() {
    let rng = &mut rand::thread_rng();
    test_encode_all_formats::<FmtConv<SetAttesterCommitteeFile>>(rng);
}
