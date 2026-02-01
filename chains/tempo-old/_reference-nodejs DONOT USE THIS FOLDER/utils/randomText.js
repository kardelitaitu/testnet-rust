import fs from 'fs';
import path from 'path';

const MNEMONIC_FILE = path.join(process.cwd(), 'utils', 'mnemonic.txt');

// Cache the word list
let cachedWords = null;

function getWords() {
    if (cachedWords) return cachedWords;

    try {
        if (!fs.existsSync(MNEMONIC_FILE)) {
            console.warn(`Warning: mnemonic.txt not found at ${MNEMONIC_FILE}`);
            return ["hello", "world", "tempo"];
        }

        const content = fs.readFileSync(MNEMONIC_FILE, 'utf-8');
        cachedWords = content
            .split('\n')
            .map(line => line.trim())
            .filter(line => line.length > 0);

        return cachedWords;
    } catch (error) {
        console.error(`Error reading mnemonic.txt: ${error.message}`);
        return ["hello", "world", "tempo"];
    }
}

/**
 * Returns a string containing a random selection of words.
 * @param {number} minWords - Minimum number of words (default: 2)
 * @param {number} maxWords - Maximum number of words (default: 3)
 * @returns {string} - Space-separated random words
 */
export function getRandomText(minWords = 2, maxWords = 3) { // Default to 2-3 words as requested
    const words = getWords();
    if (!words.length) return "hello world";

    const count = Math.floor(Math.random() * (maxWords - minWords + 1)) + minWords;
    const selected = [];

    for (let i = 0; i < count; i++) {
        const randomIndex = Math.floor(Math.random() * words.length);
        selected.push(words[randomIndex]);
    }

    return selected.join(' ');
}
