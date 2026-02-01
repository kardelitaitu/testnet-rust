
import crypto from 'crypto';

export function validatePassword(password) {
  if (!password || password.length < 8) {
    throw new Error('Password must be at least 8 characters long.');
  }
}

export function encrypt(text, password) {
  validatePassword(password);
  const salt = crypto.randomBytes(16);
  const key = crypto.scryptSync(password, salt, 32);
  const iv = crypto.randomBytes(12);
  const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
  const encrypted = Buffer.concat([cipher.update(text, 'utf8'), cipher.final()]);
  const tag = cipher.getAuthTag();
  return {
    ciphertext: encrypted.toString('hex'),
    iv: iv.toString('hex'),
    salt: salt.toString('hex'),
    tag: tag.toString('hex'),
    encryption_type: 'aes-256-gcm',
  };
}

export function decrypt(encrypted, password) {
  try {
    validatePassword(password);
    if (!encrypted.salt || !encrypted.iv || !encrypted.tag || !encrypted.ciphertext) {
      // Handle case where properties might be missing or nested under 'encrypted' key if coming from full JSON
      // But the caller should pass the inner object.
      // Let's be safe: if 'encrypted' property exists on the object, use that.
      if (encrypted.encrypted) {
        return decrypt(encrypted.encrypted, password);
      }
      throw new Error('Missing required encryption fields (salt, iv, tag, or ciphertext)');
    }

    let salt, iv, tag, ciphertext;
    try {
      salt = Buffer.from(encrypted.salt, 'hex');
      iv = Buffer.from(encrypted.iv, 'hex');
      tag = Buffer.from(encrypted.tag, 'hex');
      ciphertext = Buffer.from(encrypted.ciphertext, 'hex');
    } catch (err) {
      throw new Error('Corrupted or invalid encrypted data: Invalid hex encoding');
    }

    if (salt.length !== 16) throw new Error('Corrupted encrypted data: Invalid salt length');
    if (iv.length !== 12) throw new Error('Corrupted encrypted data: Invalid IV length');
    if (tag.length !== 16) throw new Error('Corrupted encrypted data: Invalid tag length');
    if (ciphertext.length === 0) throw new Error('Corrupted encrypted data: Empty ciphertext');

    const normalizedPassword = password.trim();
    const key = crypto.scryptSync(normalizedPassword, salt, 32);

    const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
    decipher.setAuthTag(tag);
    const decrypted = Buffer.concat([decipher.update(ciphertext), decipher.final()]);
    return decrypted.toString('utf8');
  } catch (error) {
    throw error;
  }
}
