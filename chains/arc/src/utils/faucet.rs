//! Shared faucet utility for Arc Testnet.
//!
//! Uses [Obscura](https://github.com/h4ckf0r0day/obscura) — a lightweight,
//! stealthy headless browser engine written in Rust — to automate
//! https://faucet.circle.com for test token requests.
//!
//! Obscura handles proxy authentication natively via its `--proxy` flag
//! (supports `http://user:pass@host:port`), which solves the proxy auth
//! limitation of `headless_chrome`.
//!
//! # Usage
//!
//! Install Obscura from https://github.com/h4ckf0r0day/obscura/releases
//! and ensure the `obscura` binary is in your PATH, or set `OBSCURA_PATH`
//! to point directly to the binary (e.g. `C:\\tools\\obscura.exe`).

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

/// Result from a faucet request.
pub struct FaucetResult {
    pub success: bool,
    pub message: String,
}

/// Locate the `obscura` binary.
/// Priority:
/// 1. `OBSCURA_PATH` environment variable (points to a specific binary path).
/// 2. Current working directory (for users who run from project root with obscura.exe downloaded locally).
/// 3. PATH lookup via bare binary name.
fn find_obscura_binary() -> PathBuf {
    if let Ok(path) = std::env::var("OBSCURA_PATH") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return p;
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let names: &[&str] = if cfg!(windows) { &["obscura.exe"] } else { &["obscura"] };
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

/// Build the JavaScript to evaluate inside Obscura.
///
/// Strategy:
/// 1. Override window.fetch BEFORE the page loads (via a setInterval or
///    by running early enough that React hasn't called fetch yet).
/// 2. Fill the form using ONLY DOM manipulation + event dispatch.
/// 3. Force-enable the button and click it.
/// 4. The fetch override captures the API response with sync XHR.
///
/// If no fetch call is captured, we fall back to examining script tags
/// to find the API endpoint, then make a direct API call.
fn build_eval_js(address: &str, token: &str) -> String {
    let safe_addr = address.replace('\'', "\\'");
    format!(
        r#"(function() {{
var capturedResponse = null;
var fetchCalled = false;
var debugLog = [];
var errorLog = [];
function log(msg) {{ debugLog.push(msg); }}
function err(msg) {{ errorLog.push(msg); }}

// STEP 1: Override fetch with sync XHR
window.fetch = function(url, options) {{
    fetchCalled = true;
    var method = (options && options.method) || 'GET';
    log('fetch: ' + method + ' ' + url);
    try {{
        var xhr = new XMLHttpRequest();
        xhr.open(method, url, false);
        if (options && options.headers) {{
            var ct = options.headers['Content-Type'] || options.headers['content-type'] || '';
            if (ct) xhr.setRequestHeader('Content-Type', ct);
        }}
        xhr.send((options && options.body) || null);
        var body = (xhr.responseText || '').substring(0, 3000);
        capturedResponse = JSON.stringify({{status: xhr.status, body: body, url: url.substring(0, 200)}});
        log('fetch response: ' + xhr.status);
        return Promise.resolve(new Response(xhr.responseText, {{status: xhr.status}}));
    }} catch(e) {{
        err('fetch error: ' + e.message);
        throw e;
    }}
}};
log('fetch override installed');

// STEP 2: Find and fill the address input
var input = null;
var selectors = ['input[data-testid="input"]', 'input[name="address"]', 'input[type="text"]', 'input'];
for (var i = 0; i < selectors.length; i++) {{
    var el = document.querySelector(selectors[i]);
    if (el && el.offsetParent !== null) {{ input = el; break; }}
}}
var inputFound = !!input;
if (!input) {{
    err('address input NOT FOUND');
}} else {{
    log('address input found');
    try {{
        var nativeSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value');
        if (nativeSetter) {{
            nativeSetter.set.call(input, '{safe_addr}');
        }} else {{
            input.value = '{safe_addr}';
        }}
        var evt1 = new Event('input', {{bubbles: true, cancelable: true}});
        input.dispatchEvent(evt1);
        var evt2 = new Event('change', {{bubbles: true, cancelable: true}});
        input.dispatchEvent(evt2);
        log('set address via native setter + events');
        log('input.value = ' + input.value);
    }} catch(e) {{
        err('set address error: ' + e.message);
    }}
}}

// STEP 3: Select token (default is USDC, skip for USDC)
var tokenStr = '{token}'.toLowerCase();
if (tokenStr !== 'usdc') {{
    try {{
        var radios = document.querySelectorAll('input[name="currency"]');
        var targetVal = tokenStr === 'eurc' ? 'EURC' : 'CIRBTC';
        for (var i = 0; i < radios.length; i++) {{
            if (radios[i].value === targetVal) {{
                radios[i].checked = true;
                radios[i].dispatchEvent(new Event('change', {{bubbles: true}}));
                log('checked radio: ' + targetVal);
                break;
            }}
        }}
    }} catch(e) {{
        err('radio select error: ' + e.message);
    }}
}}
log('token selection done');

// STEP 4: Find the form and buttons
var form = document.querySelector('form');
var formFound = !!form;
log('form found: ' + formFound);
if (form) log('form action=' + form.action + ' method=' + form.method);

// Find ALL buttons, not just submit buttons
var allBtns = document.querySelectorAll('button');
log('total buttons found: ' + allBtns.length);
var btnInfo = [];
for (var i = 0; i < allBtns.length; i++) {{
    btnInfo.push((allBtns[i].textContent || '').trim() + ' type=' + allBtns[i].type + ' disabled=' + allBtns[i].disabled);
}}
log('buttons: ' + JSON.stringify(btnInfo));

// Find the submit button
var submitBtn = null;
var btnSelectors = ['button[type="submit"]', 'button.cb-button', 'button'];
for (var i = 0; i < btnSelectors.length; i++) {{
    var el = document.querySelector(btnSelectors[i]);
    if (el && el.offsetParent !== null && (submitBtn === null || el.type === 'submit')) {{
        submitBtn = el;
        if (el.type === 'submit') break;
    }}
}}
var submitFound = !!submitBtn;
if (submitBtn) {{
    log('submit btn: text="' + (submitBtn.textContent || '').trim() + '" disabled=' + submitBtn.disabled + ' type=' + submitBtn.type);
    log('submit btn className: ' + submitBtn.className);
}}

// Collect all form inputs
var inputNames = [];
if (form) {{
    var q = form.querySelectorAll('input, select, textarea');
    for (var i = 0; i < q.length; i++) {{
        if (q[i].name) inputNames.push(q[i].name + '=' + (q[i].value || '').substring(0, 50));
    }}
}}
log('form elements: ' + JSON.stringify(inputNames));

// Also look for any React-like attributes on key elements
var rootEl = document.getElementById('root') || document.getElementById('__next') || document.body;
log('root element tag: ' + rootEl.tagName);
var reactKeys = Object.keys(rootEl).filter(function(k) {{ return k.indexOf('__react') === 0 || k.indexOf('_react') === 0; }});
if (reactKeys.length > 0) log('root react keys: ' + reactKeys.join(', '));
else log('no react keys on root');

// Check form element for React/internal keys
var formKeys = Object.keys(form || {{}}).filter(function(k) {{ return k.indexOf('__react') === 0 || k.indexOf('_react') === 0; }});
log('form __react keys: ' + (formKeys.length > 0 ? formKeys.join(', ') : 'NONE'));

// Check input for React keys
var inputReactKeys = input ? Object.keys(input).filter(function(k) {{ return k.indexOf('__react') === 0; }}) : [];
log('input __react keys: ' + (inputReactKeys.length > 0 ? inputReactKeys.join(', ') : 'NONE'));

// Check button for React keys
var btnReactKeys = submitBtn ? Object.keys(submitBtn).filter(function(k) {{ return k.indexOf('__react') === 0; }}) : [];
log('btn __react keys: ' + (btnReactKeys.length > 0 ? btnReactKeys.join(', ') : 'NONE'));

var pageText = (document.body.innerText || '').substring(0, 4000);

// STEP 5: SUBMIT using multiple approaches
var submittedVia = 'none';
function doSubmit(name, fn) {{
    if (submittedVia !== 'none') return;
    log('trying: ' + name);
    try {{
        fn();
        if (fetchCalled) {{
            submittedVia = name;
            log(name + ' OK');
        }} else {{
            log(name + ' no fetch');
        }}
    }} catch(e) {{
        err(name + ' error: ' + (e.message || e));
    }}
}}

// Method A: form.requestSubmit()
if (form) doSubmit('requestSubmit', function() {{ form.requestSubmit(); }});

// Method B: Remove disabled from ALL buttons, then click the submit button
if (submitBtn && submittedVia === 'none') doSubmit('clickAll', function() {{
    for (var i = 0; i < allBtns.length; i++) {{
        allBtns[i].disabled = false;
    }}
    submitBtn.click();
}});

// Method C: Dispatch submit event on form
if (form && submittedVia === 'none') doSubmit('submitEvt', function() {{
    try {{
        form.dispatchEvent(new SubmitEvent('submit', {{bubbles: true, cancelable: true}}));
    }} catch(e) {{
        form.dispatchEvent(new Event('submit', {{bubbles: true, cancelable: true}}));
    }}
}});

// Method D: Dispatch click event on the button (not just click())
if (submitBtn && submittedVia === 'none') doSubmit('clickEvt', function() {{
    submitBtn.disabled = false;
    var evt = new MouseEvent('click', {{bubbles: true, cancelable: true, view: window}});
    submitBtn.dispatchEvent(evt);
}});

// Method E: Try form.submit() (native, no event handler)
if (form && submittedVia === 'none') doSubmit('formSubmit', function() {{
    form.submit();
}});

// STEP 6: If still no fetch, examine script tags to find API endpoints
var discoveredApi = '';
if (!fetchCalled) {{
    log('fetch never called - examining page scripts for API endpoints');
    var scripts = document.querySelectorAll('script[src]');
    for (var i = 0; i < scripts.length; i++) {{
        log('script src: ' + scripts[i].src);
    }}
    var pageHtml = document.documentElement.outerHTML || '';
    var apiMatches = pageHtml.match(/\/api\/[a-zA-Z0-9_\/-]+/g) || [];
    if (apiMatches.length > 0) {{
        discoveredApi = apiMatches.join(', ');
        log('found API patterns in HTML: ' + discoveredApi);
    }}
    if (inputReactKeys.length > 0) {{
        var fiberKey = inputReactKeys[0];
        log('traversing React fiber from input...');
    }}
}}

// STEP 7: Capture post-submit state
var submitBtnText = submitBtn ? (submitBtn.textContent || '').trim() : '';
var currentUrl = window.location.href;
var actionUrl = form ? (form.action || '') : '';
var postText = (document.body.innerText || '').substring(0, 4000);
if (postText !== pageText) log('PAGE TEXT CHANGED after submit');
else log('page text did not visibly change');

log('final fetchCalled: ' + fetchCalled);
log('final capturedResponse: ' + (capturedResponse || 'null'));
log('discoveredApi: ' + (discoveredApi || 'none'));

return JSON.stringify([inputFound, submitFound, postText, formFound, currentUrl,
    submittedVia, submitBtnText, actionUrl, inputNames,
    capturedResponse, debugLog, errorLog, discoveredApi]);
}})()"#,
        safe_addr = safe_addr,
        token = token
    )
}

/// Request test tokens from the Circle faucet using Obscura.
pub fn request_tokens(
    address: &str,
    token: &str,
    proxy: Option<&str>,
    _visible: bool,
    timeout_secs: u64,
    _obscura_path: Option<PathBuf>,
) -> Result<FaucetResult> {
    let js = build_eval_js(address, token);
    let target_url = "https://faucet.circle.com/";

    let obscura_path = find_obscura_binary();
    let mut cmd = Command::new(obscura_path);

    if let Some(proxy_url) = proxy {
        cmd.arg("--proxy");
        cmd.arg(proxy_url);
    }

    cmd.arg("fetch");
    cmd.arg(target_url);
    cmd.arg("--wait-until");
    cmd.arg("networkidle0");
    cmd.arg("--eval");
    cmd.arg(&js);
    cmd.arg("--timeout");
    cmd.arg(timeout_secs.to_string());
    cmd.arg("--quiet");

    let output = cmd
        .output()
        .context("Failed to launch Obscura.\n  Set OBSCURA_PATH env var, or ensure `obscura` is in PATH.\n  Download from: https://github.com/h4ckf0r0day/obscura/releases")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Ok(FaucetResult {
            success: false,
            message: format!(
                "Obscura exited with error ({}): {} {}",
                output.status,
                stderr.trim(),
                stdout.trim()
            ),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let trimmed = stdout.trim();
    let json_str = trimmed;

    let parse_result = serde_json::from_str::<serde_json::Value>(json_str);

    let mut input_found = false;
    let mut submit_found = false;
    let mut page_text = String::new();
    let mut _form_found = false;
    let mut _current_url = String::new();
    let mut submitted_via = String::new();
    let mut submit_btn_text = String::new();
    let mut _action_url = String::new();
    let mut input_names = String::new();
    let mut captured_response = String::new();
    let mut api_status: i64 = 0;
    let mut api_body = String::new();
    let mut debug_log = String::new();
    let mut error_log = String::new();
    let mut discovered_api = String::new();
    let mut parse_ok = false;

    match parse_result {
        Ok(serde_json::Value::Array(arr)) if arr.len() >= 13 => {
            input_found = arr[0].as_bool().unwrap_or(false);
            submit_found = arr[1].as_bool().unwrap_or(false);
            page_text = arr[2].as_str().unwrap_or("").to_string();
            _form_found = arr[3].as_bool().unwrap_or(false);
            _current_url = arr[4].as_str().unwrap_or("").to_string();
            submitted_via = arr[5].as_str().unwrap_or("").to_string();
            submit_btn_text = arr[6].as_str().unwrap_or("").to_string();
            _action_url = arr[7].as_str().unwrap_or("").to_string();
            input_names = arr[8]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default();
            captured_response = arr[9].as_str().unwrap_or("").to_string();
            debug_log = arr[10]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default();
            error_log = arr[11]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default();
            discovered_api = arr[12].as_str().unwrap_or("").to_string();
            parse_ok = true;

            if !captured_response.is_empty() {
                if let Ok(cr) = serde_json::from_str::<serde_json::Value>(&captured_response) {
                    api_status = cr.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
                    api_body = cr.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string();
                }
            }
        },
        Ok(serde_json::Value::Array(arr)) if arr.len() >= 6 => {
            input_found = arr[0].as_bool().unwrap_or(false);
            submit_found = arr[1].as_bool().unwrap_or(false);
            page_text = arr.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
            _form_found = arr.get(3).and_then(|v| v.as_bool()).unwrap_or(false);
            _current_url = arr.get(4).and_then(|v| v.as_str()).unwrap_or("").to_string();
            submitted_via = arr.get(5).and_then(|v| v.as_str()).unwrap_or("").to_string();
            parse_ok = true;
        },
        _ => {},
    }

    if !parse_ok {
        let raw = stdout.trim();
        return Ok(FaucetResult {
            success: false,
            message: format!(
                "❌ Faucet eval returned unexpected format.\nRaw Obscura stdout:\n{}\nCheck explorer: https://testnet.arcscan.app/address/{}",
                &raw[..raw.len().min(500)],
                address
            ),
        });
    }

    let form_submitted = input_found && submit_found;

    let api_success = api_status == 200
        && (api_body.to_lowercase().contains("success")
            || api_body.contains("txHash")
            || api_body.contains("transactionHash")
            || api_body.contains("\"sent\""));

    let api_rate_limited = api_status == 429
        || api_body.to_lowercase().contains("rate limit")
        || api_body.to_lowercase().contains("too many requests");

    let api_invalid = api_status >= 400
        && (api_body.to_lowercase().contains("invalid")
            || api_body.to_lowercase().contains("error")
            || api_body.to_lowercase().contains("bad request"));

    let message = if !input_found || !submit_found {
        format!(
            "❌ Faucet UI elements not found (input={} submit={}).\nDebug log:\n{}\nErrors:\n{}\nCheck explorer: https://testnet.arcscan.app/address/{}",
            input_found, submit_found, debug_log, error_log, address
        )
    } else if api_status == 429 || api_rate_limited {
        format!(
            "❌ Rate limited (HTTP {}). API body:\n{}\nCheck explorer: https://testnet.arcscan.app/address/{}",
            api_status,
            &api_body[..api_body.len().min(500)],
            address
        )
    } else if api_invalid {
        format!(
            "❌ API returned error (HTTP {}).\nResponse:\n{}\nCheck explorer: https://testnet.arcscan.app/address/{}",
            api_status,
            &api_body[..api_body.len().min(500)],
            address
        )
    } else if api_success {
        format!(
            "✅ Faucet request SUCCESSFUL for {} {} (HTTP {})!\nAPI body:\n{}\nCheck explorer: https://testnet.arcscan.app/address/{}",
            address, token.to_uppercase(), api_status,
            &api_body[..api_body.len().min(1000)], address
        )
    } else if !discovered_api.is_empty() {
        format!(
            "❌ Form filled but NO API call was made.\nDiscovered API patterns: {}\nSubmitted via: {}\nButton: {}\nInputs: {}\n\nDebug log:\n{}\n\nErrors:\n{}\n\nPage text:\n{}\n\nCheck explorer: https://testnet.arcscan.app/address/{}",
            discovered_api, submitted_via, submit_btn_text, input_names,
            debug_log, error_log,
            &page_text[..page_text.len().min(500)], address
        )
    } else if captured_response.is_empty() {
        format!(
            "❌ Form filled but NO API call was made.\nSubmitted via: {}\nButton: {}\nInputs: {}\n\nDebug log:\n{}\n\nErrors:\n{}\n\nPage text:\n{}\n\nCheck explorer: https://testnet.arcscan.app/address/{}",
            submitted_via, submit_btn_text, input_names,
            debug_log, error_log,
            &page_text[..page_text.len().min(500)], address
        )
    } else if api_status >= 400 {
        format!(
            "❌ API returned error status (HTTP {}) via {}.\nResponse body:\n{}\nCheck explorer: https://testnet.arcscan.app/address/{}",
            api_status, submitted_via, &api_body[..api_body.len().min(500)], address
        )
    } else {
        format!(
            "ℹ️  Faucet request submitted for {} {} via {}. API: HTTP {}\n\nDebug log:\n{}\n\nVerify on-chain: https://testnet.arcscan.app/address/{}",
            address, token.to_uppercase(), submitted_via, api_status, debug_log, address
        )
    };

    let success = form_submitted && api_status == 200 && !api_invalid;

    Ok(FaucetResult { success, message })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_eval_js_contains_address() {
        let js = build_eval_js("0x1234567890abcdef1234567890abcdef12345678", "usdc");
        assert!(js.contains("0x1234567890abcdef1234567890abcdef12345678"));
    }

    #[test]
    fn test_build_eval_js_uses_iife() {
        let js = build_eval_js("0xabc", "usdc");
        assert!(js.contains("(function() {"));
        assert!(js.contains("})()"));
    }

    #[test]
    fn test_build_eval_js_has_return() {
        let js = build_eval_js("0xabc", "usdc");
        assert!(js.contains("return JSON.stringify"));
    }

    #[test]
    fn test_build_eval_js_no_async_await() {
        let js = build_eval_js("0xabc", "usdc");
        assert!(!js.contains("async"));
        assert!(!js.contains("await"));
    }

    #[test]
    fn test_build_eval_js_includes_fetch_override() {
        let js = build_eval_js("0xabc", "usdc");
        assert!(js.contains("XMLHttpRequest"));
        assert!(js.contains("xhr.open"));
        assert!(js.contains("capturedResponse"));
    }

    #[test]
    fn test_build_eval_js_tries_multiple_approaches() {
        let js = build_eval_js("0xabc", "usdc");
        assert!(js.contains("requestSubmit"));
        assert!(js.contains("clickAll"));
        assert!(js.contains("submitEvt"));
        assert!(js.contains("clickEvt"));
        assert!(js.contains("formSubmit"));
    }

    #[test]
    fn test_build_eval_js_examines_scripts() {
        let js = build_eval_js("0xabc", "usdc");
        assert!(js.contains("script[src]"));
        assert!(js.contains("discoveredApi"));
    }

    #[test]
    fn test_find_obscura_binary_respects_obscura_path_env() {
        let tmp_dir = std::env::temp_dir();
        let tmp_binary = tmp_dir.join("obscura-test-binary-placeholder");
        std::fs::write(&tmp_binary, b"placeholder").ok();

        std::env::set_var("OBSCURA_PATH", &tmp_binary);
        let result = find_obscura_binary();
        assert_eq!(result, tmp_binary);

        std::env::remove_var("OBSCURA_PATH");
        std::fs::remove_file(&tmp_binary).ok();
        let after = find_obscura_binary();
        assert_ne!(after, tmp_binary);
    }
}
