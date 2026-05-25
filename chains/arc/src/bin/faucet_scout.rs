//! Scout: Dump reCAPTCHA script details + try longer timeout.
use core_logic::ProxyManager;
use std::path::PathBuf;
use std::process::Command;

fn find_obscura_binary() -> PathBuf {
    if let Ok(path) = std::env::var("OBSCURA_PATH") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return p;
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let names: &[&str] = if cfg!(windows) {
            &["obscura.exe"]
        } else {
            &["obscura"]
        };
        for name in names {
            let candidate = cwd.join(name);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    if cfg!(windows) {
        PathBuf::from("obscura.exe")
    } else {
        PathBuf::from("obscura")
    }
}

fn main() {
    let proxy_url = {
        let proxies = ProxyManager::load_proxies().expect("load proxies");
        if !proxies.is_empty() {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let p = &proxies[rng.gen_range(0..proxies.len())];
            let url = if let (Some(user), Some(pass)) = (&p.username, &p.password) {
                let host = p.url.trim_start_matches("http://");
                format!("http://{}:{}@{}", user, pass, host)
            } else {
                p.url.clone()
            };
            println!("Proxy: {}", url.split('@').next_back().unwrap_or("..."));
            Some(url)
        } else {
            None
        }
    };

    let js = r#"(function() {
var logs = [];
var debug = [];

// 1. Dump ALL script tags with recaptcha in src or content
var allScripts = document.querySelectorAll('script');
debug.push('total scripts: ' + allScripts.length);
for (var i = 0; i < allScripts.length; i++) {
    var s = allScripts[i];
    if (s.src) {
        if (s.src.indexOf('google') >= 0 || s.src.indexOf('gstatic') >= 0 || s.src.indexOf('recaptcha') >= 0 || s.src.indexOf('captcha') >= 0) {
            logs.push('SCRIPT[' + i + '] src: ' + s.src);
            logs.push('  async: ' + s.async + ' defer: ' + s.defer);
        }
    } else if (s.textContent && s.textContent.length > 0) {
        if (s.textContent.indexOf('recaptcha') >= 0 || s.textContent.indexOf('grecaptcha') >= 0) {
            logs.push('INLINE_SCRIPT[' + i + ']: first 1000 chars:');
            logs.push(s.textContent.substring(0, 1000));
        }
    }
}

// 2. Check for grecaptcha under various names with pre-defined onload
try {
    if (typeof window.__onRecaptchaLoad === 'undefined') {
        window.__onRecaptchaLoad = function() {
            logs.push('onRecaptchaLoad CALLED!');
            if (window.grecaptcha) {
                logs.push('grecaptcha available in callback!');
                logs.push('grecaptcha type: ' + typeof window.grecaptcha);
                logs.push('grecaptcha keys: ' + Object.keys(window.grecaptcha).join(','));
            }
        };
    }
    logs.push('registered onRecaptchaLoad callback');
} catch(e) {}

// 3. Check if grecaptcha exists now
logs.push('grecaptcha NOW: ' + typeof window.grecaptcha);
if (window.grecaptcha) {
    logs.push('grecaptcha keys: ' + Object.keys(window.grecaptcha).join(','));
    logs.push('grecaptcha.ready: ' + typeof window.grecaptcha.ready);
    logs.push('grecaptcha.execute: ' + typeof window.grecaptcha.execute);
}

// 4. Check if __NEXT_DATA__ now exists with longer wait
try {
    var nd = window.__NEXT_DATA__;
    logs.push('__NEXT_DATA__: ' + (nd ? JSON.stringify(nd).substring(0, 500) : 'undefined'));
} catch(e) {}

// 5. Check page for any token-like fields
logs.push('=== FORM STATE ===');
var form = document.querySelector('form');
if (form) {
    var inputs = form.querySelectorAll('input, textarea');
    for (var i = 0; i < inputs.length; i++) {
        var inp = inputs[i];
        logs.push('input[' + i + '] name=' + (inp.name || '') + ' type=' + inp.type + ' value=' + (inp.value || '').substring(0, 100));
    }
}

// 6. Check all meta tags and data attributes for recaptcha token
var metas = document.querySelectorAll('meta');
for (var i = 0; i < metas.length; i++) {
    if (metas[i].getAttribute('name') && metas[i].getAttribute('name').indexOf('captcha') >= 0) {
        logs.push('meta recaptcha: ' + metas[i].getAttribute('content'));
    }
}
var allEls = document.querySelectorAll('[data-recaptcha], [data-sitekey], [data-token]');
for (var i = 0; i < allEls.length; i++) {
    logs.push('data-recaptcha element: ' + allEls[i].tagName + ' data-sitekey=' + (allEls[i].getAttribute('data-sitekey') || '') + ' data-token=' + (allEls[i].getAttribute('data-token') || ''));
}

// 7. Check for any postMessage handlers
logs.push('=== NETWORK/RESOURCES ===');
try {
    var perfEntries = performance.getEntriesByType('resource') || [];
    var captchaResources = [];
    for (var i = 0; i < perfEntries.length; i++) {
        if (perfEntries[i].name.indexOf('google') >= 0 || perfEntries[i].name.indexOf('gstatic') >= 0 || perfEntries[i].name.indexOf('recaptcha') >= 0) {
            captchaResources.push(perfEntries[i].name.substring(0, 150));
        }
    }
    logs.push('captcha resources loaded: ' + JSON.stringify(captchaResources));
} catch(e) {}

return JSON.stringify({logs: logs, debug: debug});
})()"#;

    let mut cmd = Command::new(find_obscura_binary());
    if let Some(p) = &proxy_url {
        cmd.arg("--proxy").arg(p);
    }
    cmd.arg("fetch")
        .arg("https://faucet.circle.com/")
        .arg("--wait-until")
        .arg("networkidle2")
        .arg("--eval")
        .arg(js)
        .arg("--timeout")
        .arg("45");

    let output = cmd.output().expect("Failed to run Obscura");
    if !output.status.success() {
        println!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.trim().is_empty() {
        println!("Empty stdout");
        return;
    }
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
        if let Some(logs) = val.get("logs").and_then(|v| v.as_array()) {
            for entry in logs {
                if let Some(s) = entry.as_str() {
                    println!("{}", s);
                }
            }
        }
    } else {
        println!("Raw:\n{}", stdout);
    }
}
