const { Web3 } = require('web3');
const web3 = new Web3('http://127.0.0.1:8545');
const fs = require('fs');


const abi = [
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "_chainId",
        "type": "uint256"
      },
      {
        "components": [
          {
            "internalType": "uint64",
            "name": "batchNumber",
            "type": "uint64"
          },
          {
            "internalType": "bytes32",
            "name": "batchHash",
            "type": "bytes32"
          },
          {
            "internalType": "uint64",
            "name": "indexRepeatedStorageChanges",
            "type": "uint64"
          },
          {
            "internalType": "uint256",
            "name": "numberOfLayer1Txs",
            "type": "uint256"
          },
          {
            "internalType": "bytes32",
            "name": "priorityOperationsHash",
            "type": "bytes32"
          },
          {
            "internalType": "bytes32",
            "name": "l2LogsTreeRoot",
            "type": "bytes32"
          },
          {
            "internalType": "uint256",
            "name": "timestamp",
            "type": "uint256"
          },
          {
            "internalType": "bytes32",
            "name": "commitment",
            "type": "bytes32"
          }
        ],
        "internalType": "struct IExecutor.StoredBatchInfo",
        "name": "",
        "type": "tuple"
      },
      {
        "components": [
          {
            "internalType": "uint64",
            "name": "batchNumber",
            "type": "uint64"
          },
          {
            "internalType": "uint64",
            "name": "timestamp",
            "type": "uint64"
          },
          {
            "internalType": "uint64",
            "name": "indexRepeatedStorageChanges",
            "type": "uint64"
          },
          {
            "internalType": "bytes32",
            "name": "newStateRoot",
            "type": "bytes32"
          },
          {
            "internalType": "uint256",
            "name": "numberOfLayer1Txs",
            "type": "uint256"
          },
          {
            "internalType": "bytes32",
            "name": "priorityOperationsHash",
            "type": "bytes32"
          },
          {
            "internalType": "bytes32",
            "name": "bootloaderHeapInitialContentsHash",
            "type": "bytes32"
          },
          {
            "internalType": "bytes32",
            "name": "eventsQueueStateHash",
            "type": "bytes32"
          },
          {
            "internalType": "bytes",
            "name": "systemLogs",
            "type": "bytes"
          },
          {
            "internalType": "bytes",
            "name": "pubdataCommitments",
            "type": "bytes"
          }
        ],
        "internalType": "struct IExecutor.CommitBatchInfo[]",
        "name": "_newBatchesData",
        "type": "tuple[]"
      }
    ],
    "name": "commitBatchesSharedBridge",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  }
]

const contract = new web3.eth.Contract(abi);

function hexToUtf8(hex) {
  // Remove the '0x' prefix if present
  if (hex.startsWith('0x')) {
    hex = hex.slice(2);
  }

  // Ensure the hex string has an even length
  if (hex.length % 2 !== 0) {
    throw new Error('Invalid hex string length');
  }

  // Convert hex string to a byte array
  const bytes = [];
  for (let i = 0; i < hex.length; i += 2) {
    bytes.push(parseInt(hex.substr(i, 2), 16));
  }

  // Convert byte array to UTF-8 string
  const utf8String = new TextDecoder('utf-8').decode(new Uint8Array(bytes));
  return utf8String;
}

function uint8ArrayToHex(uint8Array) {
  return Array.from(uint8Array)
      .map(byte => byte.toString(16).padStart(2, '0')) // Convert each byte to a 2-digit hex value
      .join(''); // Join all the hex values into a single string
}

async function getTransactions(validatorTimelockAddress, commitBatchesSharedBridge_functionSelector) {
  const latestBlock = await web3.eth.getBlockNumber();
  let jsonArray = [];
  for (let i = 0; i <= latestBlock; i++) {
    const block = await web3.eth.getBlock(i, true);
    await Promise.all(block.transactions.map(async tx => {
      if (tx.to && (tx.to.toLowerCase() == validatorTimelockAddress.toLowerCase())) { 
        const input = tx.input;
        const txSelector = input.slice(0, 10);
        if (txSelector == commitBatchesSharedBridge_functionSelector) {
          const functionAbi = contract.options.jsonInterface.find(item => item.name === 'commitBatchesSharedBridge');
          const decodedParams = web3.eth.abi.decodeParameters(
            functionAbi.inputs,
            input.slice(10) // Remove the function selector (first 10 characters of the calldata)
          );
          commitment = hexToUtf8(decodedParams._newBatchesData[0].pubdataCommitments.slice(4));
          let blob = await get(commitment);
          const blobHex = uint8ArrayToHex(blob);
          jsonArray.push({
              commitment: commitment,
              blob: blobHex
          });
        }
      }
    }));
  }
  const jsonString = JSON.stringify(jsonArray, null, 2);
  fs.writeFileSync("blob_data.json", jsonString, 'utf8');
}

async function get(commitment) {
  try {
      const url = `http://localhost:4242/get/0x${commitment}`;

      const response = await fetch(url);

      if (response.ok) {
          // Expecting the response body to be binary data
          const body = await response.arrayBuffer();
          return new Uint8Array(body);
      } else {
          return []; // Return empty array if the response is not successful
      }
  } catch (error) {
      // Handle any errors
      console.error('Error fetching data:', error);
      throw error; // Re-throw the error or return an empty array, depending on requirements
  }
}

function getArguments() {
  const args = process.argv.slice(2); // Get arguments after the first two (which are node and script path)

  let validatorTimelockAddress = null;
  let commitBatchesSharedBridge_functionSelector = null;
  args.forEach(arg => {
      const [key, value] = arg.split('=');
      if (key === 'validatorTimelockAddress') {
        validatorTimelockAddress = value;
      } else if (key === 'commitBatchesSharedBridge_functionSelector') {
        commitBatchesSharedBridge_functionSelector = value;
      }
  });

  // Check if both arguments are provided
  if (!validatorTimelockAddress || !commitBatchesSharedBridge_functionSelector) {
      console.error('Usage: node getallblobs.js validatorTimelockAddress=<validatorTimelockAddress> commitBatchesSharedBridge_functionSelector=<commitBatchesSharedBridge_functionSelector>');
      process.exit(1); // Exit with error
  }

  return { validatorTimelockAddress, commitBatchesSharedBridge_functionSelector };
}

function main() {
  // Values for local node:
  // validatorTimelockAddress = check in zk init
  // commitBatchesSharedBridge_functionSelector = "0x6edd4f12"
  const { validatorTimelockAddress, commitBatchesSharedBridge_functionSelector } = getArguments();
  getTransactions(validatorTimelockAddress, commitBatchesSharedBridge_functionSelector);
}

main();

//Contracts being called in L1:
//Chain Admin
//Diamond Proxy
//Governance
//Validator Timelock //6edd4f12 function selector commitBatchesSharedBridge
//BridgeHub Proxy
//Transparent Proxy
//Create2 Factory
//Contracts Create2Factory (verifier)
