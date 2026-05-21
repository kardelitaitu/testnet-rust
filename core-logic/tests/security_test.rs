use core_logic::security::SecurityUtils;

/// Known test vector from wallet 0001.json (password: diNingrat@10)
const CIPHERTEXT: &str = "5881cf5ae82f6824abd49bb2b41791419b207f110aa7193ee7c238e94ba5bdb54dcb0e85611a8d9367bdecc1f89e0c494ef5f4f00ddb0a7e6413572a86db6f96c6478f31b54484289dd31bb8765daf5116dc6ed8b4411dfe2bfe6466e2ab0b5575bbe78b26bc7f2063812a31756f06a81d75bf11f7536cd2f24d183749890bf890a2a9c932ba7b4b35739423c4ecf5f4041274eafb638750779690a687abd3303ec1cea9e941feefc89df2b8e662d5ea74ef78dc42e51b5b48c84a30fc3932a50b049a972a83f8df7ad37ee8ca9a3aa98ece05a613e475d2afc40cb68337ca85e3fe3955b32e1cfd9a150f4d15bb3723f0ccbcf8574ce830308f23cedfcb9d0e6e404a23af1dfe02818d99fba21c5e2296f2e178b02e60dc4ddd6a2873c0a26b86153b26a6757ea981943669ad030f84fad8c44e5a4c3764b500a8274aeebfe46cc34e36aa592bc6c8eab3b227a9370af6f8b706420f81ba5c20966a333f284ab9cf7681c741cc44282e7e6a233ef4d561932465d6b633c4d7fe045c8477ff17485eb36d6fbd2a4476066092353b0ccfa4c065b6a0a6e9531a6abddfe14b43c1e7b9ad0fc45ae6fd713825ca59a4dffadbfabf2b1435a27dc2e876e46b16834081886d11d19347c95c3061a881e36f04636565e412f7f0a05bab4dbaff051451dd1a2ea8a58653196fef0669f197e011eafa958935f46ba9d3c0050eb0b4b9b41d5abf97e70a1010c691c4e9a635642c3a86caa15d53e807748aeb341e15634050abdb97c570b732f9c76f059fb48a9d2c9688c88e4093e7ec163c3504f0553c3f0b2849e3d4ff311dde05cfea07404a79aed0c727191013f64c137c353c5687816b4c85cfbf398be2404471011dd5840c7a23d8d41986fe68b3525cef8af77d9407e4d0e69d375aded3c6b803fe9ff8fbd90443b343d00054ef701128c2d647f40421fc61c701f5ae34f608e1659cd2cfb8b9ff6b4b7ce6badb882350e269c7b153011587afaecb67fc7eba56f6981ce588ee23aee66027f92e65ac0a204890bc6161b1542185df6f02555efdf64381d4a038c4bd550abdbb2303ed0848b9ef15de1bb1965f5a96b9c80277fee22c0ac1ce305a05957f0e6b21578acdc252843cc2cff851474854789b1791340d4046b3ee08594bff095f8d121ac607a96acec7412569bde934242595d9dedd7369594310998b1b4d901d1b5430b1d35d87fd8e1401afebe40fb165962bfa7f5d75dbb3caddce1c82ccdfe01ae395a36c44bfa116df446262335eb5ea3b32ece7bb2a808abf7ad096d0905d3fe50ccacb6efc920f223672addc4a08019f33f1b98ef940cd00ab02be1d268f94c18ff23d9638bdc148d91c3c7d2e71f4d8e02270dfc2af14f23107ad1c24fbe42736f1400f8073ed20a611be2c78ee535f7eb2797df05cc4b69f6e1d2e7580b55c772ee42f00f332e8e6954543d38f06e08a30d1a9d494d52e3c9e1af05b83d352e5c6fc5812e8d66a9507c75d97f3bb0782c366993ef821dcf62f90b3c1d70de00c19045b4fbe4a6cacd39f4727972035751ef56fdaa7fb8d0ebefca214b154d4381283a80172d936679f50c810e211b571a80da2992f747cb611ebba3f6b2a6fcbcac6ebc51ee37272a3a38095fed75f55014426de47c9413d9e892e0661ff27b2d3e8d82475b361eeb04dd365270f531ffe843a9bedca4aa42c6b7b747958671a43f1cf7af0903f6b81c2ebf5ccc9b0";
const IV: &str = "28132d0007de4ed107d97ce0";
const SALT: &str = "b9f9af427bd53566ed4d9cf3a2016639";
const TAG: &str = "04cf5ab8da75d5e94486af0fe9015015";

#[test]
fn test_decrypt_with_correct_password() {
    let result = SecurityUtils::decrypt_components(CIPHERTEXT, IV, SALT, TAG, "diNingrat@10");
    assert!(
        result.is_ok(),
        "Decryption should succeed with correct password"
    );

    let decrypted = result.unwrap();
    // Should be valid JSON containing an Ethereum private key
    assert!(
        decrypted.contains("private_key")
            || decrypted.contains("evm_private_key")
            || decrypted.contains("address")
    );
    // Should contain the known address
    let known_address_lower = "d7d2e492e6dda0013e9062f00327a06fdb722488";
    assert!(decrypted.to_lowercase().contains(known_address_lower));
}

#[test]
fn test_decrypt_with_wrong_password() {
    let result = SecurityUtils::decrypt_components(CIPHERTEXT, IV, SALT, TAG, "wrong_password");
    assert!(
        result.is_err(),
        "Decryption should fail with wrong password"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("Decryption failed") || err.contains("decrypt"));
}

#[test]
fn test_decrypt_with_empty_password() {
    let result = SecurityUtils::decrypt_components(CIPHERTEXT, IV, SALT, TAG, "");
    assert!(
        result.is_err(),
        "Decryption should fail with empty password"
    );
}

#[test]
fn test_decrypt_invalid_hex() {
    let result = SecurityUtils::decrypt_components("not_hex", IV, SALT, TAG, "diNingrat@10");
    assert!(result.is_err(), "Should fail on invalid hex");
}

#[test]
fn test_decrypt_invalid_iv_hex() {
    let result =
        SecurityUtils::decrypt_components(CIPHERTEXT, "not_hex", SALT, TAG, "diNingrat@10");
    assert!(result.is_err(), "Should fail on invalid IV hex");
}

#[test]
fn test_decrypt_invalid_salt_hex() {
    let result = SecurityUtils::decrypt_components(CIPHERTEXT, IV, "not_hex", TAG, "diNingrat@10");
    assert!(result.is_err(), "Should fail on invalid salt hex");
}

#[test]
fn test_decrypt_short_ciphertext() {
    let result = SecurityUtils::decrypt_components("00", IV, SALT, TAG, "diNingrat@10");
    assert!(result.is_err(), "Should fail on too-short ciphertext");
}

#[test]
#[should_panic(expected = "assertion")]
fn test_decrypt_empty_components() {
    // hex::decode("") succeeds (empty vec), but AES-GCM requires 12-byte nonce
    // This panics at the crypto library level, not our code
    let _ = SecurityUtils::decrypt_components("", "", "", "", "");
}

#[test]
fn test_decrypt_different_password_case() {
    // Case sensitivity test - same password but different case
    let result = SecurityUtils::decrypt_components(CIPHERTEXT, IV, SALT, TAG, "DININGRAT@10");
    assert!(result.is_err(), "Should fail with wrong case password");
}
