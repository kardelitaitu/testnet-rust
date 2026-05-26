use core_logic::RpcManager;
use std::sync::Arc;
use std::thread;

#[test]
fn test_rpc_manager_concurrent_get_endpoint() {
    let urls = vec![
        "https://rpc1.com".to_string(),
        "https://rpc2.com".to_string(),
        "https://rpc3.com".to_string(),
    ];
    let mgr = Arc::new(RpcManager::new(1, &urls));
    let mut handles = vec![];

    for _ in 0..10 {
        let mgr_clone = Arc::clone(&mgr);
        let urls_clone = urls.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                let ep = mgr_clone.get_endpoint().unwrap();
                assert!(urls_clone.contains(&ep.url));
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_rpc_manager_concurrent_health_updates() {
    let urls = vec![
        "https://rpc1.com".to_string(),
        "https://rpc2.com".to_string(),
    ];
    let mgr = Arc::new(RpcManager::new(1, &urls));
    let mut handles = vec![];

    for i in 0..10 {
        let mgr_clone = Arc::clone(&mgr);
        let url = urls[i % urls.len()].clone();
        handles.push(thread::spawn(move || {
            for j in 0..100 {
                if j % 2 == 0 {
                    mgr_clone.record_success(&url);
                } else {
                    mgr_clone.record_failure(&url);
                }
                mgr_clone.record_latency(&url, j as u64);
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }
    
    // Final state should be stable (no panics)
    assert!(mgr.endpoints_count() == 2);
}

#[test]
fn test_rpc_manager_fastest_with_latency_updates() {
    let urls = vec![
        "http://a.com".to_string(),
        "http://b.com".to_string(),
    ];
    let mgr = Arc::new(RpcManager::new(1, &urls));
    
    mgr.record_latency("http://a.com", 100);
    mgr.record_latency("http://b.com", 200);
    
    assert_eq!(mgr.get_fastest().unwrap().url, "http://a.com");
    
    // Concurrent update
    let mgr_clone = Arc::clone(&mgr);
    let handle = thread::spawn(move || {
        mgr_clone.record_latency("http://a.com", 300);
    });
    handle.join().unwrap();
    
    assert_eq!(mgr.get_fastest().unwrap().url, "http://b.com");
}
