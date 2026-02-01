import { ethers } from 'ethers';

// The error selector
const errorSelector = '0xaa4bc69a';

// The parameter from error data
const errorParam = '0x91d14854';

console.log('Error Selector:', errorSelector);
console.log('Error Parameter (hex):', errorParam);
console.log('Error Parameter (decimal):', parseInt(errorParam, 16));

// Check if this matches any amount calculations
console.log('\nWith 6 decimals:');
console.log('  ', parseInt(errorParam, 16) / 1e6, 'tokens');

console.log('\nWith 18 decimals:');
console.log('  ', parseInt(errorParam, 16) / 1e18, 'tokens');

// Reverse check: what would 1000 tokens be?
console.log('\n1000 tokens with 6 decimals:', (1000 * 1e6).toString(16));
console.log('1000 tokens with 18 decimals:', (1000n * 10n ** 18n).toString(16));

// Check function selectors
console.log('\nFunction Selectors:');
const mintSig = 'mint(address,uint256)';
const mintSelector = ethers.id(mintSig).substring(0, 10);
console.log('mint(address,uint256):', mintSelector);
