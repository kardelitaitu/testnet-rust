# Framework Logging Standard: The "Gold Standard"

## 1. Overview
The framework employs a unified, high-precision logging format designed for maximum scannability and auditability during high-throughput operations. This standard is currently implemented in `sepolia-overlayer` and serves as the reference for all future chain implementations.

## 2. Log Line Structure
Every task execution log must follow this exact sequence and padding:

`HH:MM:SS [WK:XXX][WL:YYYY][P:PPP] STATUS  [CC/LL][TaskName] Message`

### 2.1 Component Breakdown
| Component | Format | Description |
| :--- | :--- | :--- |
| **Timestamp** | `HH:MM:SS` | Local time (via `chrono::Local`). |
| **Worker ID** | `[WK:001]` | 3-digit padded (supports 000-999). |
| **Wallet ID** | `[WL:0005]` | 4-digit padded (supports 0000-9999). |
| **Proxy ID** | `[P:003]` | 3-digit padded (1-indexed) or `---` if no proxy. |
| **Status** | `OK     ` | 7-character fixed width, left-aligned. |
| **Progress** | `[01/10]` | Current count vs Daily limit (optional for daily tasks). |
| **Task Name** | `[name]` | The name of the task being executed. |
| **Message** | `text` | The result message or error description. |

## 3. Status Labels (Fixed 7-char width)
Labels must be consistent to ensure the message column always aligns perfectly.

*   `OK     ` : Task succeeded.
*   `RETRY  ` : Task failed but will be retried (Transient error).
*   `TIMEOUT` : Task exceeded the configured execution timeout.
*   `LIMIT  ` : Wallet has reached its daily transaction capacity.
*   `ERROR  ` : Fatal or non-retryable error.

## 4. Implementation Guidelines
- **Target**: Use the `task_result` tracing target to separate execution logs from system internal logs.
- **Level**:
    - Use `info!` for `OK`, `RETRY`, and `LIMIT`.
    - Use `error!` for `TIMEOUT` and `ERROR` to ensure visibility on the console.
- **Color**: Success should be Green, Failure should be Red (handled by the framework's `TerminalFormatter`).

## 5. Example Output
```text
14:32:17 [WK:001][WL:0005][P:003] OK      [01/10][check_balance] Balance: 0.5 ETH
14:32:18 [WK:001][WL:0005][P:---] RETRY   [00/10][mint_token] RPC timeout
14:32:45 [WK:002][WL:0012][P:042] TIMEOUT  [03/10][swap_exact] Task exceeded 300s
14:33:01 [WK:000][WL:0008][P:---] LIMIT   [10/10][all_tasks] Completed for today
```

## 6. Rationale
*   **Vertical Alignment**: Fixed-width fields allow the human eye to scan down specific columns (like status or task name) without horizontal jitter.
*   **Auditability**: Padded IDs ensure logs from different workers can be sorted and filtered using standard CLI tools like `grep`, `awk`, or `sort`.
*   **Visual Priority**: Forcing `TIMEOUT` to the `error!` level ensures that network/RPC degradation is immediately visible to the operator.
