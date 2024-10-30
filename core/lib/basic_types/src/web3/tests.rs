use super::*;

#[test]
fn block_id_can_be_deserialized() {
    let block_id: BlockId = serde_json::from_str("\"latest\"").unwrap();
    assert_eq!(block_id, BlockId::Number(BlockNumber::Latest));
    let block_id: BlockId = serde_json::from_str("\"pending\"").unwrap();
    assert_eq!(block_id, BlockId::Number(BlockNumber::Pending));
    let block_id: BlockId = serde_json::from_str("\"earliest\"").unwrap();
    assert_eq!(block_id, BlockId::Number(BlockNumber::Earliest));
    let block_id: BlockId = serde_json::from_str("\"0x12\"").unwrap();
    assert_eq!(
        block_id,
        BlockId::Number(BlockNumber::Number(U64::from(0x12)))
    );

    let block_id: BlockId = serde_json::from_str(
        r#"{ "blockHash": "0x4242424242424242424242424242424242424242424242424242424242424242" }"#,
    )
    .unwrap();
    assert_eq!(block_id, BlockId::Hash(H256::repeat_byte(0x42)));
}

#[test]
fn block_can_be_deserialized() {
    let post_dencun = r#"
        {
            "baseFeePerGas": "0x3e344c311",
            "blobGasUsed": "0xc0000",
            "difficulty": "0x0",
            "excessBlobGas": "0x4b40000",
            "extraData": "0xd883010d0d846765746888676f312e32302e34856c696e7578",
            "gasLimit": "0x1c9c380",
            "gasUsed": "0x96070e",
            "hash": "0x2c77691707319e5b8a5afde5b75c77ec0ccb1f556c1ccd9e0e49ef5795bc9015",
            "logsBloom": "0x08a212844c2050e028d95b42b718a550c7215100101028408e90520820c81c1171400212a0200483f89c89010a0604934650804fc0a283a06d011041a22c000296e0880394850081700ac41a156812a59ea5de0518a43bcb0000a865c242c42348f570a9a3704350c484424d0400ab04000805e0840094680028a218c0099dc2ca42b01c63224045510375050860308894480901e8a6d0e818035158805218400e493001c9a8060a205c1112611442040804041fc00088cca49e262b068000349cf611ebc61c1b00220282150c16096c1370921412420815008398200041721d12d2988ea090d0429208010564809845080808707a1d3052f343020e88091221",
            "miner": "0x0000000000000000000000000000000000000000",
            "mixHash": "0xcae6acf2f1e34499c234db81aff35299979046ab0e544276005899ce5f320858",
            "nonce": "0x0000000000000000",
            "number": "0x51fca7",
            "parentBeaconBlockRoot": "0x08750d6ce3bf47639efc14c43f44fc1c018d593f1b48aacd222668422ed289ce",
            "parentHash": "0x2f3978cb0dec7ecbb01fff2be87b101cf61e9bfb31d64c15a19aca7c4f74c87e",
            "receiptsRoot": "0xdad010222b2ce0862e928402b2d10d9651d78d9276c8ad30bc9c0793aba9dd18",
            "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
            "size": "0xa862",
            "stateRoot": "0xf77a819b26fef9fe5c7cd9511b0fff950725e878cb311914de162be068fb8c70",
            "timestamp": "0x65ddadfc",
            "totalDifficulty": "0x3c656d23029ab0",
            "transactions": ["0x8660a77cc4f13681e039b8f980579c53f522ba13aad64481a5bdfe5f62b54bbb", "0xab5820f72a6374db701a5e27668f14dac867ade71806bb754b561a63a9441df4"],
            "transactionsRoot": "0xd9f4b814ee427db492c4fcc249df665b616db61bd423413ad7ffd9b464aced1e",
            "uncles": [],
            "withdrawals": [{
                "address": "0xe276bc378a527a8792b353cdca5b5e53263dfb9e",
                "amount": "0x24e0",
                "index": "0x2457a70",
                "validatorIndex": "0x3df"
            }, {
                "address": "0xe276bc378a527a8792b353cdca5b5e53263dfb9e",
                "amount": "0x24e0",
                "index": "0x2457a71",
                "validatorIndex": "0x3e1"
            }],
            "withdrawalsRoot": "0x576ea3c5cc0bf9d73f64e9786e7b44595a62a76a54989acc8ae84b2015fde929"
        }"#;
    let block: Block<H256> = serde_json::from_str(post_dencun).unwrap();
    assert_eq!(block.excess_blob_gas, Some(U64::from(0x4b40000)));

    let pre_dencun = r#"
        {
            "baseFeePerGas": "0x910fe39fd",
            "difficulty": "0x0",
            "extraData": "0x546974616e2028746974616e6275696c6465722e78797a29",
            "gasLimit": "0x1c9c380",
            "gasUsed": "0x10d5e31",
            "hash": "0x40a92d343f724efbc2c85b710a433391b9102813560704ec7db89572a3b23451",
            "logsBloom": "0x15a15083ea8148119f3845a4a7f01c291a1393153ec36e21ecdc60cffea60c55c16397c190e48eb7efa27f96500959541a91c251a8e0aeca5ed0942a4dec8a118b6a25bdd9248c6b8b53556ed62c0be8daec00f984ec196f5faa4f52cae5ceb76ade34648e73ac6371bed5ecd1491fc7083a2d75ea88e7e0f7448d56969cb8069fc09a768d1a93f1833f81c1050e36e465220cf99f9502bbc4e0b5475d52b8e1ea819ecb7bacb8baeb16d8f59b9904cc171b970d0834c8fecd6291df1514dae1ff474e939c5af95773a013f6926d1dd5915591d28e18201daf707923f417a9ead1796959c3cfe6facf0c85a7a620aac8a44616eb484db2d78039fb606de4549d",
            "miner": "0x4838b106fce9647bdf1e7877bf73ce8b0bad5f97",
            "mixHash": "0x041736203c26f01d68a6a3b5728205cc27cb1674fdad8cc85dd722933483b446",
            "nonce": "0x0000000000000000",
            "number": "0x126c531",
            "parentHash": "0x57535175149b25b4258e6de43e7484f6b6f144dcddbb0add5c2f25e700c57508",
            "receiptsRoot": "0x8cc94702aca57e058aaa5da92e7e776f76ac67191a638c5664844deb59616c8f",
            "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
            "size": "0x3bfca",
            "stateRoot": "0xbeb859bb91bc80792f6640ff469837927dc5a7ad5393484b2e1b536f3815af2f",
            "timestamp": "0x65ddae8b",
            "totalDifficulty": "0xc70d815d562d3cfa955",
            "transactions": ["0x61b183ea82664e6afb05d3fa692f71561381e5c9280e80062770ff88d4c3c8be", "0x3dd322f75e4315dd265c22a8ae3e51bd32253f43664d1dc4d2afb68746da7e70"],
            "transactionsRoot": "0xba2b8ff471608f8877abc6d06f7f599e3c942671b11598b0595fc13185681458",
            "uncles": [],
            "withdrawals": [{
                "address": "0xb9d7934878b5fb9610b3fe8a5e441e8fad7e293f",
                "amount": "0x1168dd0",
                "index": "0x22d6606",
                "validatorIndex": "0x5c797"
            }, {
                "address": "0xb9d7934878b5fb9610b3fe8a5e441e8fad7e293f",
                "amount": "0x117bae7",
                "index": "0x22d6607",
                "validatorIndex": "0x5c798"
            }],
            "withdrawalsRoot": "0xf3386eae1beb91726fe62e2aba4b975ed4f3be2222c739bf2b4d72dd20324a8b"
        }"#;
    let block: Block<H256> = serde_json::from_str(pre_dencun).unwrap();
    assert!(block.excess_blob_gas.is_none());
}

#[test]
fn test_bytes_serde_bincode() {
    let original = Bytes(vec![0, 1, 2, 3, 4]);
    let encoded: Vec<u8> = bincode::serialize(&original).unwrap();
    let decoded: Bytes = bincode::deserialize(&encoded).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bytes_serde_bincode_snapshot() {
    let original = Bytes(vec![0, 1, 2, 3, 4]);
    let encoded: Vec<u8> = vec![5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4];
    let decoded: Bytes = bincode::deserialize(&encoded).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bytes_serde_json() {
    let original = Bytes(vec![0, 1, 2, 3, 4]);
    let encoded = serde_json::to_string(&original).unwrap();
    let decoded: Bytes = serde_json::from_str(&encoded).unwrap();
    assert_eq!(original, decoded);
}
