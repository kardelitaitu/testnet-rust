use aes_gcm::{
    aead::{Aead, NewAead}, // NewAead for 0.9/0.4
    Aes256Gcm,
    Nonce,
};
use anyhow::{Context, Result};
use scrypt;
// use std::fs;
use hex;

pub struct SecurityUtils;

impl SecurityUtils {
    pub fn decrypt_components(
        ciphertext_hex: &str,
        iv_hex: &str,
        salt_hex: &str,
        tag_hex: &str,
        password: &str,
    ) -> Result<String> {
        let ciphertext = hex::decode(ciphertext_hex).context("Invalid ciphertext hex")?;
        let iv = hex::decode(iv_hex).context("Invalid IV hex")?;
        let salt = hex::decode(salt_hex).context("Invalid salt hex")?;
        let mut tag = hex::decode(tag_hex).context("Invalid tag hex")?;

        // Derive Key using Scrypt (Node.js crypto.scryptSync defaults: N=16384, r=8, p=1)
        // Rust scrypt Params: log_n (14 -> 16384), r (8), p (1)
        let params = scrypt::Params::new(14, 8, 1, 32)
            .map_err(|e| anyhow::anyhow!("Invalid scrypt params: {}", e))?;
        let mut key = [0u8; 32];
        scrypt::scrypt(password.as_bytes(), &salt, &params, &mut key)
            .map_err(|e| anyhow::anyhow!("Scrypt failed: {}", e))?;

        let cipher = Aes256Gcm::new(&key.into());
        let nonce = Nonce::from_slice(&iv);

        let mut full_payload = ciphertext.clone();
        full_payload.append(&mut tag);

        let plaintext = cipher
            .decrypt(nonce, full_payload.as_ref())
            .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

        let text = String::from_utf8(plaintext).context("Decrypted data is not valid UTF-8")?;
        Ok(text)
    }
    // Keeping old one for reference or other tools, but likely unused now
    pub fn decrypt_file(_path: &str, _password: &str) -> Result<String> {
        Err(anyhow::anyhow!("Use decrypt_components for JSON wallets"))
    }
}
