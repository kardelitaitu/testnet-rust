const fs = require('fs');
const crypto = require('crypto');
// require('dotenv').config();

function decrypt(encrypted, password) {
    try {
        if (!encrypted.salt || !encrypted.iv || !encrypted.tag || !encrypted.ciphertext) {
            throw new Error('Missing required encryption fields');
        }
        const salt = Buffer.from(encrypted.salt, 'hex');
        const iv = Buffer.from(encrypted.iv, 'hex');
        const tag = Buffer.from(encrypted.tag, 'hex');
        const ciphertext = Buffer.from(encrypted.ciphertext, 'hex');

        // Default params for scryptSync: N=16384, r=8, p=1
        const key = crypto.scryptSync(password, salt, 32);

        console.log("Derived Key (Hex):", key.toString('hex'));

        const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
        decipher.setAuthTag(tag);
        const decrypted = Buffer.concat([decipher.update(ciphertext), decipher.final()]);
        return decrypted.toString('utf8');
    } catch (error) {
        if (error.message.includes('Unsupported state or unable to authenticate data')) {
            throw new Error('Authentication failed: Likely incorrect password');
        }
        throw error;
    }
}

try {
    const walletPath = 'wallet-json/0001.json';
    const content = fs.readFileSync(walletPath, 'utf8');
    const json = JSON.parse(content);

    const password = "password"; // Hardcoded for verification
    console.log(`Using password: '${password}'`);

    if (json.encrypted) {
        console.log("Attempting decryption...");
        const decrypted = decrypt(json.encrypted, password);
        console.log("SUCCESS!");
        console.log("Decrypted Text:", decrypted);
    } else {
        console.log("File is not encrypted.");
    }

} catch (e) {
    console.error("FAILED:", e.message);
}
