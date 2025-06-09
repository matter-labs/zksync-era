//! Different interfaces exposed by the `IExecutor.sol`.

use anyhow::Context;
use bellman::bn256::Bn256;
use circuit_definitions::circuit_definitions::aux_layer::ZkSyncSnarkWrapperCircuit;
use ruint::aliases::{B160, U256};
use zk_ee::utils::Bytes32;
use zk_os_basic_system::system_implementation::system::{BatchOutput, BatchPublicInput};
use zksync_types::commitment::{L1BatchWithMetadata, ZkosCommitment};
use zksync_types::H256;
use bellman::plonk::better_better_cs::proof::{Proof as PlonkProof, Proof};

pub mod commit;
pub mod methods;
pub mod structures;


// todo: will be refactored after zkos schema migration
// we'll probably compute the commitement earlier in the process and save in the DB

pub fn batch_public_input(
    prev_batch: &L1BatchWithMetadata,
    current_batch: &L1BatchWithMetadata,
) -> BatchPublicInput {
    let prev_commitment = ZkosCommitment::from(prev_batch);
    let current_commitment = ZkosCommitment::from(current_batch);

    BatchPublicInput {
        state_before: h256_to_bytes32(prev_commitment.state_commitment()),
        state_after: h256_to_bytes32(current_commitment.state_commitment()),
        batch_output: zkos_commitment_to_vm_batch_output(&current_commitment).hash().into(),
    }
}

pub fn zkos_commitment_to_vm_batch_output(commitment: &ZkosCommitment) -> BatchOutput {
    let (_, operator_da_input_header_hash) = commitment.calculate_operator_da_input();

    BatchOutput {
        chain_id: U256::from(commitment.chain_id),
        first_block_timestamp: commitment.block_timestamp,
        last_block_timestamp: commitment.block_timestamp,
        used_l2_da_validator_address: B160::default(),
        pubdata_commitment: h256_to_bytes32(operator_da_input_header_hash),
        number_of_layer_1_txs: U256::from(commitment.number_of_layer1_txs),
        priority_operations_hash: h256_to_bytes32(commitment.priority_operations_hash()),
        l2_logs_tree_root: h256_to_bytes32(commitment.l2_to_l1_logs_root_hash),
        upgrade_tx_hash: Bytes32::zero(),
    }
}

pub fn batch_output_hash_as_register_values(public_input: &BatchPublicInput) -> [u32; 8] {
    public_input.hash().chunks_exact(4).map(|chunk| {
        u32::from_le_bytes(chunk.try_into().expect("Slice with incorrect length"))
    }).collect::<Vec<u32>>().try_into().expect("Hash should be exactly 32 bytes long")
}

pub fn deserialize_snark_plank_proof(bytes: Vec<u8>) -> anyhow::Result<PlonkProof<Bn256, ZkSyncSnarkWrapperCircuit>> {
    let str_ = r#"{
  "n": 16777215,
  "inputs": [
    [
      11872606732827553975,
      14989528687795465396,
      8148398114634408091,
      3333022410
    ]
  ],
  "state_polys_commitments": [
    {
      "x": [
        15372723922664760995,
        10082718601311422026,
        2568070440048972941,
        1838755782666936762
      ],
      "y": [
        17725565987870479505,
        6839250965405869654,
        12310598836773584497,
        3284994451949039827
      ],
      "infinity": false
    },
    {
      "x": [
        15100425418557633918,
        8731701128120302212,
        13178037096171446686,
        1811972755604218134
      ],
      "y": [
        9259989087901977880,
        9182664494747280041,
        14535211449993274135,
        2822190207340090336
      ],
      "infinity": false
    },
    {
      "x": [
        4778238844465151491,
        879122591453380852,
        4984284412661676317,
        3144919266703243278
      ],
      "y": [
        16849990960826890603,
        15559173573533597601,
        6299945368789531586,
        1437462067763943004
      ],
      "infinity": false
    },
    {
      "x": [
        4199088857303397453,
        3877043745433819440,
        11128501316477944153,
        1431696220159048853
      ],
      "y": [
        6097894500076643869,
        10859160776728583885,
        15685583140194587489,
        1159199063275282067
      ],
      "infinity": false
    }
  ],
  "witness_polys_commitments": [],
  "copy_permutation_grand_product_commitment": {
    "x": [
      9469121784764537579,
      7424617606070110691,
      10447174869876630957,
      1951523616708206749
    ],
    "y": [
      12022928682846859093,
      138329095171106013,
      7615368428440429768,
      1502561477938555661
    ],
    "infinity": false
  },
  "lookup_s_poly_commitment": {
    "x": [
      7625761954461532877,
      9449730835235991214,
      7421322619958222824,
      1003992444375804947
    ],
    "y": [
      8951416478512794605,
      6510969769216547133,
      12790881449753747409,
      1783555448888223781
    ],
    "infinity": false
  },
  "lookup_grand_product_commitment": {
    "x": [
      3156198788831169950,
      11435623233509147533,
      16553317808928570378,
      3261510437334699704
    ],
    "y": [
      368788142812465629,
      16468294187974492498,
      3355035515881803229,
      2444349482935336466
    ],
    "infinity": false
  },
  "quotient_poly_parts_commitments": [
    {
      "x": [
        895794990808690891,
        4246339763313724833,
        4934160894456126254,
        2298644686526258563
      ],
      "y": [
        18162345842809166490,
        14970424645121686290,
        8901264169940445428,
        1595404106570299686
      ],
      "infinity": false
    },
    {
      "x": [
        5604830029921038255,
        2218864800875647381,
        2038800620354497306,
        1974857024262017074
      ],
      "y": [
        3865226359929048537,
        7303717838452720903,
        3421703454093216259,
        2034570613401975284
      ],
      "infinity": false
    },
    {
      "x": [
        7417560469262272283,
        9720043245052487445,
        13154538581593408617,
        2943930169013917207
      ],
      "y": [
        7948199887368319540,
        7650163378855826251,
        17027806961054196148,
        1464157744926329130
      ],
      "infinity": false
    },
    {
      "x": [
        8695642721756568057,
        9908413985542753110,
        12867810574060617608,
        2629640311060706543
      ],
      "y": [
        3188732464597769220,
        14237097652203558962,
        11375699507978727974,
        3319406637327070218
      ],
      "infinity": false
    }
  ],
  "state_polys_openings_at_z": [
    [
      6364346816216754610,
      7904804730850841218,
      11157418205334537454,
      2784884235634018346
    ],
    [
      18296690313123475180,
      7125585600313325731,
      16736372985086601031,
      1218653766746994169
    ],
    [
      2430233342085714715,
      2097080893766777783,
      9544167741792208803,
      434879722925308195
    ],
    [
      10915443826152898609,
      12557417589926120616,
      15757340836042951693,
      172573984322466874
    ]
  ],
  "state_polys_openings_at_dilations": [
    [
      1,
      3,
      [
        11291344216689482404,
        9432840965208270208,
        2834193035354261222,
        245868165409468180
      ]
    ]
  ],
  "witness_polys_openings_at_z": [],
  "witness_polys_openings_at_dilations": [],
  "gate_setup_openings_at_z": [],
  "gate_selectors_openings_at_z": [
    [
      0,
      [
        1965896187677760654,
        8352931443145045302,
        2148499315770284261,
        2217506682295421474
      ]
    ]
  ],
  "copy_permutation_polys_openings_at_z": [
    [
      11458107000384069516,
      13535174016334140138,
      15847510160659205792,
      258885055172470264
    ],
    [
      8598139643332967660,
      16058128692080130899,
      10921885005805572911,
      2918433150006702532
    ],
    [
      15404891006679543889,
      593475075980390671,
      14837936504674267084,
      3024327662336694007
    ]
  ],
  "copy_permutation_grand_product_opening_at_z_omega": [
    12134543580051263088,
    7288015921973474318,
    5670753864364531885,
    3315881089886373156
  ],
  "lookup_s_poly_opening_at_z_omega": [
    8973910826035063785,
    14684614584330502538,
    5497197681128860510,
    2354200031061182921
  ],
  "lookup_grand_product_opening_at_z_omega": [
    17904947612396054991,
    9362949780317534616,
    10757324943019943313,
    2195554909641104301
  ],
  "lookup_t_poly_opening_at_z": [
    8456249827593938873,
    1234165155599273087,
    9887111933613208119,
    2172077417962766960
  ],
  "lookup_t_poly_opening_at_z_omega": [
    13753114069401296037,
    11583754536765954764,
    4435769715756658270,
    743860029346599426
  ],
  "lookup_selector_poly_opening_at_z": [
    10286029189727001347,
    241456087691450143,
    1478869968948212238,
    2468181383688837309
  ],
  "lookup_table_type_poly_opening_at_z": [
    18276092432879893460,
    14516582613709371805,
    16053272031786292030,
    2576218950652678269
  ],
  "quotient_poly_opening_at_z": [
    13677892492406326509,
    4896107805267367632,
    15219516163755339806,
    1322979343221955267
  ],
  "linearization_poly_opening_at_z": [
    16072488143601725971,
    7718682231904246983,
    5820793448580868777,
    1532857392531937984
  ],
  "opening_proof_at_z": {
    "x": [
      15243791852454099562,
      6473534083863607074,
      13027352464771866523,
      1324331783773116390
    ],
    "y": [
      16887065798414293270,
      16086927388516016412,
      9491656518588712693,
      1837313553358526684
    ],
    "infinity": false
  },
  "opening_proof_at_z_omega": {
    "x": [
      7450028393434659052,
      9799977868196720608,
      13627303016331904797,
      1485354676650120922
    ],
    "y": [
      11104336734366610835,
      6546531704907475294,
      13782237372082789201,
      316478750300382877
    ],
    "infinity": false
  }
}"#;
    let r: Proof<Bn256, ZkSyncSnarkWrapperCircuit> = serde_json::from_str(str_).expect("");
    Ok(r)
}

pub fn h256_to_bytes32(input: H256) -> Bytes32 {
    let mut new = Bytes32::zero();
    new.as_u8_array_mut().copy_from_slice(input.as_bytes());
    new
}
