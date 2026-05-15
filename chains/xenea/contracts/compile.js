const solc = require('solc');
const fs = require('fs');
const crypto = require('crypto');
const path = require('path');

const CONTRACT_NAME = process.argv[2] || 'Counter';
const CONTRACT_FILE = `${CONTRACT_NAME}.sol`;
const HASH_FILE = `${CONTRACT_NAME}.sol.hash`;
const BYTECODE_FILE = `${CONTRACT_NAME}_bytecode.txt`;
const ABI_FILE = `${CONTRACT_NAME}_abi.txt`;

if (!fs.existsSync(CONTRACT_FILE)) {
    console.error(`Error: ${CONTRACT_FILE} not found.`);
    process.exit(1);
}

function computeHash(content) {
    return crypto.createHash('sha256').update(content).digest('hex');
}

function shouldCompile(content) {
    if (!fs.existsSync(HASH_FILE) || !fs.existsSync(BYTECODE_FILE) || !fs.existsSync(ABI_FILE)) {
        return true;
    }
    const storedHash = fs.readFileSync(HASH_FILE, 'utf8');
    const currentHash = computeHash(content);
    return storedHash !== currentHash;
}

const content = fs.readFileSync(CONTRACT_FILE, 'utf8');

if (!shouldCompile(content)) {
    console.log(`No changes detected in ${CONTRACT_FILE}. Skipping compilation.`);
    process.exit(0);
}

console.log(`Compiling ${CONTRACT_FILE}...`);

const input = {
    language: 'Solidity',
    sources: {
        [CONTRACT_FILE]: {
            content: content
        }
    },
    settings: {
        optimizer: {
            enabled: true,
            runs: 200
        },
        evmVersion: 'paris',
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

const contract = output.contracts[CONTRACT_FILE][CONTRACT_NAME];
const bytecode = contract.evm.bytecode.object;
const abi = JSON.stringify(contract.abi);

fs.writeFileSync(BYTECODE_FILE, bytecode);
fs.writeFileSync(ABI_FILE, abi);
fs.writeFileSync(HASH_FILE, computeHash(content));

console.log(`Files written: ${BYTECODE_FILE}, ${ABI_FILE}, ${HASH_FILE}`);
