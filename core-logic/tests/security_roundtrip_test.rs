use core_logic::SecurityUtils;
use aes_gcm::{
    aead::{Aead, NewAead},
    Aes256Gcm, Nonce,
};
use scrypt;
use hex;

#[test]
fn test_security_utils_roundtrip_manual() {
    // This test manually performs the encryption logic that SecurityUtils expects
    // to verify the decryption works correctly.
    
    let password = "test-password-123";
    let plaintext = "secret data to encrypt";
    let salt = [1u8; 32];
    let iv = [2u8; 12];
    
    // Derive key using the same params as SecurityUtils
    let params = scrypt::Params::new(14, 8, 1, 32).unwrap();
    let mut key = [0u8; 32];
    scrypt::scrypt(password.as_bytes(), &salt, &params, &mut key).unwrap();
    
    // Encrypt
    let cipher = Aes256Gcm::new(&key.into());
    let nonce = Nonce::from_slice(&iv);
    
    // In SecurityUtils, it expects ciphertext + tag appended
    // aes_gcm's encrypt method returns ciphertext with tag appended by default
    let encrypted_data = cipher.encrypt(nonce, plaintext.as_bytes()).unwrap();
    
    // Split ciphertext and tag
    let tag_pos = encrypted_data.len() - 16;
    let ciphertext = &encrypted_data[..tag_pos];
    let tag = &encrypted_data[tag_pos..];
    
    // Convert to hex for SecurityUtils
    let ciphertext_hex = hex::encode(ciphertext);
    let iv_hex = hex::encode(iv);
    let salt_hex = hex::encode(salt);
    let tag_hex = hex::encode(tag);
    
    // Decrypt using SecurityUtils
    let decrypted = SecurityUtils::decrypt_components(
        &ciphertext_hex,
        &iv_hex,
        &salt_hex,
        &tag_hex,
        password
    ).expect("Decryption should succeed");
    
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_security_utils_wrong_password() {
    let password = "correct-password";
    let wrong_password = "wrong-password";
    let plaintext = "secret";
    let salt = [1u8; 32];
    let iv = [2u8; 12];
    
    let params = scrypt::Params::new(14, 8, 1, 32).unwrap();
    let mut key = [0u8; 32];
    scrypt::scrypt(password.as_bytes(), &salt, &params, &mut key).unwrap();
    
    let cipher = Aes256Gcm::new(&key.into());
    let nonce = Nonce::from_slice(&iv);
    let encrypted_data = cipher.encrypt(nonce, plaintext.as_bytes()).unwrap();
    
    let tag_pos = encrypted_data.len() - 16;
    let ciphertext_hex = hex::encode(&encrypted_data[..tag_pos]);
    let iv_hex = hex::encode(iv);
    let salt_hex = hex::encode(salt);
    let tag_hex = hex::encode(&encrypted_data[tag_pos..]);
    
    let result = SecurityUtils::decrypt_components(
        &ciphertext_hex,
        &iv_hex,
        &salt_hex,
        &tag_hex,
        wrong_password
    );
    
    assert!(result.is_err(), "Decryption with wrong password should fail");
}
