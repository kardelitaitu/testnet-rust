const solc = require('solc');
const fs = require('fs');

const sources = {
    'StoragePattern.sol': fs.readFileSync('StoragePattern.sol', 'utf8'),
    'CustomErrorTest.sol': fs.readFileSync('CustomErrorTest.sol', 'utf8'),
    'RevertReason.sol': fs.readFileSync('RevertReason.sol', 'utf8'),
    'AssertFail.sol': fs.readFileSync('AssertFail.sol', 'utf8'),
    'AnonymousEvent.sol': fs.readFileSync('AnonymousEvent.sol', 'utf8'),
    'IndexedTopics.sol': fs.readFileSync('IndexedTopics.sol', 'utf8'),
    'LargeEventData.sol': fs.readFileSync('LargeEventData.sol', 'utf8'),
    'MemoryExpansion.sol': fs.readFileSync('MemoryExpansion.sol', 'utf8'),
    'CalldataSize.sol': fs.readFileSync('CalldataSize.sol', 'utf8'),
    'GasStipend.sol': fs.readFileSync('GasStipend.sol', 'utf8')
};

const input = {
    language: 'Solidity',
    sources: {
        'StoragePattern.sol': { content: sources['StoragePattern.sol'] },
        'CustomErrorTest.sol': { content: sources['CustomErrorTest.sol'] },
        'RevertReason.sol': { content: sources['RevertReason.sol'] },
        'AssertFail.sol': { content: sources['AssertFail.sol'] },
        'AnonymousEvent.sol': { content: sources['AnonymousEvent.sol'] },
        'IndexedTopics.sol': { content: sources['IndexedTopics.sol'] },
        'LargeEventData.sol': { content: sources['LargeEventData.sol'] },
        'MemoryExpansion.sol': { content: sources['MemoryExpansion.sol'] },
        'CalldataSize.sol': { content: sources['CalldataSize.sol'] },
        'GasStipend.sol': { content: sources['GasStipend.sol'] }
    },
    settings: {
        outputSelection: {
            '*': {
                '*': ['*']
            }
        }
    }
};

const output = JSON.parse(solc.compile(JSON.stringify(input)));

if (output.errors) {
    output.errors.forEach(err => {
        if (err.severity === 'error') {
            console.error(err.formattedMessage);
        } else {
            console.log(err.formattedMessage);
        }
    });
}

// Helper to write bytecode
function writeBytecode(fileName, contractName, outputName) {
    if (output.contracts[fileName] && output.contracts[fileName][contractName]) {
        const bytecode = output.contracts[fileName][contractName].evm.bytecode.object;
        fs.writeFileSync(outputName, '0x' + bytecode);
        console.log(`Wrote bytecode for ${contractName} to ${outputName}`);
    } else {
        console.error(`Contract ${contractName} not found in output for ${fileName}`);
    }
}

writeBytecode('StoragePattern.sol', 'StoragePattern', 'bytecode_t44.txt');
writeBytecode('CustomErrorTest.sol', 'CustomErrorTest', 'bytecode_t45.txt');
writeBytecode('RevertReason.sol', 'RevertReason', 'bytecode_t46.txt');
writeBytecode('AssertFail.sol', 'AssertFail', 'bytecode_t47.txt');
writeBytecode('AnonymousEvent.sol', 'AnonymousEvent', 'bytecode_t48.txt');
writeBytecode('IndexedTopics.sol', 'IndexedTopics', 'bytecode_t49.txt');
writeBytecode('LargeEventData.sol', 'LargeEventData', 'bytecode_t50.txt');
writeBytecode('MemoryExpansion.sol', 'MemoryExpansion', 'bytecode_t51.txt');
writeBytecode('CalldataSize.sol', 'CalldataSize', 'bytecode_t52.txt');
writeBytecode('GasStipend.sol', 'GasStipend', 'bytecode_t53.txt');
