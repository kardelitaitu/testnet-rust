const solc = require('solc');
const fs = require('fs');
const path = require('path');

const CONTRACT_FILE_ARG = process.argv[2];

if (!CONTRACT_FILE_ARG) {
    console.error("Usage: node compile.js <ContractPath>");
    process.exit(1);
}

const CONTRACT_PATH = path.resolve(CONTRACT_FILE_ARG);
const CONTRACT_NAME = path.basename(CONTRACT_PATH, '.sol');
const PARENT_DIR = path.dirname(CONTRACT_PATH);

if (!fs.existsSync(CONTRACT_PATH)) {
    console.error(`Error: File ${CONTRACT_PATH} not found.`);
    process.exit(1);
}

const content = fs.readFileSync(CONTRACT_PATH, 'utf8');

const input = {
    language: 'Solidity',
    sources: {
        [path.basename(CONTRACT_PATH)]: {
            content: content
        }
    },
    settings: {
        optimizer: {
            enabled: true,
            runs: 200
        },
        outputSelection: {
            '*': {
                '*': ['*']
            }
        }
    }
};

const output = JSON.parse(solc.compile(JSON.stringify(input)));

if (output.errors) {
    let hasError = false;
    output.errors.forEach(err => {
        console.error(err.formattedMessage);
        if (err.severity === 'error') hasError = true;
    });
    if (hasError) process.exit(1);
}

const contract = output.contracts[path.basename(CONTRACT_PATH)][CONTRACT_NAME];
if (!contract) {
    console.error("Contract not found in output.");
    process.exit(1);
}

const bytecode = contract.evm.bytecode.object;
const abi = JSON.stringify(contract.abi);

// Output as JSON to stdout for Rust to parse
console.log(JSON.stringify({
    abi: abi,
    bin: bytecode
}));
