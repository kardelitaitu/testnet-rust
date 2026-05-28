use criterion::{criterion_group, criterion_main, Criterion};
use core_logic::{RpcManager, WalletManager, ChainType, DatabaseManager, AsyncDbConfig, FallbackStrategy, QueuedTaskResult};
use std::sync::Arc;
use tokio::runtime::Runtime;
use aes_gcm::{aead::{Aead, NewAead}, Aes256Gcm, Nonce};
use hex;

fn bench_rpc_manager(c: &mut Criterion) {
    let urls = vec![
        "https://rpc1.com".to_string(),
        "https://rpc2.com".to_string(),
        "https://rpc3.com".to_string(),
    ];
    let mgr = RpcManager::new(1, &urls);

    c.bench_function("rpc_manager_get_endpoint", |b| {
        b.iter(|| {
            let _ = mgr.get_endpoint();
        })
    });
    
    mgr.record_latency("https://rpc1.com", 50);
    mgr.record_latency("https://rpc2.com", 20);
    mgr.record_latency("https://rpc3.com", 100);
    
    c.bench_function("rpc_manager_get_fastest", |b| {
        b.iter(|| {
            let _ = mgr.get_fastest();
        })
    });
}

fn generate_encrypted_wallet(password: &str) -> String {
    let salt = [0u8; 32];
    let iv = [0u8; 12];
    // SecurityUtils uses hardcoded N=16384 (log_n=14)
    let params = scrypt::Params::new(14, 8, 1, 32).unwrap();
    let mut key = [0u8; 32];
    scrypt::scrypt(password.as_bytes(), &salt, &params, &mut key).unwrap();
    
    let cipher = Aes256Gcm::new(&key.into());
    let nonce = Nonce::from_slice(&iv);
    // Plaintext must be valid JSON for DecryptedWallet
    let plaintext = r#"{"mnemonic":"test benchmark","evm_address":"0x123","evm_private_key":"0xabc"}"#;
    let encrypted = cipher.encrypt(nonce, plaintext.as_bytes()).unwrap();
    
    let tag_pos = encrypted.len() - 16;
    let ciphertext = &encrypted[..tag_pos];
    let tag = &encrypted[tag_pos..];
    
    format!(r#"{{
        "address": "0x123",
        "encrypted": {{
            "ciphertext": "{}",
            "iv": "{}",
            "salt": "{}",
            "tag": "{}"
        }}
    }}"#, hex::encode(ciphertext), hex::encode(iv), hex::encode(salt), hex::encode(tag))
}

fn bench_wallet_manager(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let wallet_json = generate_encrypted_wallet("pwd");
    std::fs::write(dir.path().join("w0.json"), wallet_json).unwrap();
    
    let mgr = Arc::new(WalletManager::with_wallet_dir(dir.path()).unwrap());
    let rt = Runtime::new().unwrap();
    
    c.bench_function("wallet_manager_cache_hit", |b| {
        // Prime the cache
        let _ = rt.block_on(mgr.get_wallet_for_chain(0, Some("pwd"), ChainType::Evm));
        
        b.iter(|| {
            let res = rt.block_on(mgr.get_wallet_for_chain(0, Some("pwd"), ChainType::Evm));
            assert!(res.is_ok());
        })
    });

    c.bench_function("wallet_manager_full_load_decryption", |b| {
        b.iter(|| {
            // Note: clear_cache is async, we must await it
            rt.block_on(mgr.clear_cache());
            let res = rt.block_on(mgr.get_wallet_for_chain(0, Some("pwd"), ChainType::Evm));
            assert!(res.is_ok());
        })
    });
}

fn bench_database_async_enqueue(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("bench.db");
    
    let db = rt.block_on(DatabaseManager::new_with_async(
        db_path.to_str().unwrap(),
        AsyncDbConfig {
            channel_capacity: 10000,
            batch_size: 100,
            flush_interval_ms: 1000,
        },
        FallbackStrategy::Drop
    )).unwrap();
    
    let result_template = QueuedTaskResult {
        worker_id: "BENCH".into(),
        wallet_address: "0x123".into(),
        task_name: "task".into(),
        success: true,
        message: "integrated".into(),
        duration_ms: 10,
        timestamp: 123456789,
    };

    c.bench_function("database_async_enqueue_overhead", |b| {
        b.iter(|| {
            let _ = db.queue_task_result(result_template.clone());
        })
    });
    
    let db_owned = Arc::try_unwrap(Arc::new(db)).unwrap_or_else(|_| panic!("Failed to unwrap DB"));
    rt.block_on(db_owned.shutdown()).ok();
}

criterion_group!(benches, bench_rpc_manager, bench_wallet_manager, bench_database_async_enqueue);
criterion_main!(benches);
