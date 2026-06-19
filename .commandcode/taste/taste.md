# Taste (Continuously Learned by [CommandCode][cmd])

[cmd]: https://commandcode.ai/

# sepolia-funder
- Maximum transaction confirmation timeout should be 60 seconds (not 600s). Confidence: 0.85
- Bump GAS PRICE (gwei), not gas limit, when speeding up txs. Gas limit stays at 21000 for plain ETH transfers. Confidence: 0.80
- Before any replacement/bump tx, check the proxy's current on-chain balance and compute the MAX gas price affordable for sending the full forward value. Use min(desired_bump, max_affordable) — never send a replacement the proxy can't afford. Confidence: 0.80
- Do NOT bump gas on every heartbeat — the replacement carries the same value transfer, so cumulative bumps exhaust the proxy's gas budget. Heartbeats should only log progress; reserve gas bumps for a single last-ditch at the confirmation deadline. Confidence: 0.80

# communication
- Use terse, lowercase, direct tone matching the user. Push forward with the work without asking for permission or summarizing excessively. Confidence: 0.65
- When the user pushes back on a design (e.g. "i think we should check the proxy balance..."), do not argue — re-read the relevant code, identify the root cause they spotted, and implement the fix they described. Confidence: 0.75

# workflow
- After implementing code changes, run `cargo test` and `cargo clippy -- -D warnings` before reporting done. The user expects tests to pass and zero clippy warnings. Confidence: 0.85
- When the user says "proceed carefully" before a change, implement changes incrementally and verify with tests at each step rather than batching everything. Confidence: 0.65
- When capturing a "before" snapshot for a delta/comparison check, fetch it at the START of the operation, not the end. Capturing both endpoints at the end gives a delta of ~0 and makes the check a silent no-op — exactly the kind of bug the user will spot and call out. Confidence: 0.90
- The "Address X received N ETH" success log must reflect ACTUAL on-chain delivery (delta), not the planned/calculated amount. If the verification said delivery failed, the log should say "MUST have received" or similar — never assert success on a value that wasn't actually observed on-chain. Confidence: 0.85
- After a confirmed tx receipt, validate receipt.to matches the expected destination and receipt.from matches the expected sender. A mismatch indicates the RPC returned a stale or wrong receipt and the user has zero tolerance for silently reporting success on a misattributed tx. Confidence: 0.80
