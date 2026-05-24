use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn fund_help_works() {
    let mut cmd = Command::cargo_bin("sepolia-funder").unwrap();
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Multi-hop obfuscated ETH funder"));
}

#[test]
fn fund_dry_run_fails_gracefully_when_no_wallets() {
    // The binary requires a wallet directory before it can do anything useful.
    // This test ensures the CLI entry point and early error path remain stable.
    let mut cmd = Command::cargo_bin("sepolia-funder").unwrap();

    cmd.arg("--dry-run")
        .arg("--yes")
        .arg("--config")
        .arg("config.toml")
        .arg("--min-balance")
        .arg("0")
        .arg("--max-balance")
        .arg("100");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No wallets found"));
}

#[test]
fn fund_dry_run_flag_appears_in_help() {
    // Verify the --dry-run flag is properly registered in the CLI interface.
    let mut cmd = Command::cargo_bin("sepolia-funder").unwrap();
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("print plan but don't send"));
}

#[test]
fn fund_dry_run_accepts_all_optional_flags() {
    // Ensure --dry-run can be combined with all common optional flags without
    // argument-parsing errors. The actual execution will fail due to missing
    // wallets, but the argument validation should pass.
    let mut cmd = Command::cargo_bin("sepolia-funder").unwrap();

    cmd.arg("--dry-run")
        .arg("--yes")
        .arg("--config")
        .arg("config.toml")
        .arg("--min-balance")
        .arg("0.5")
        .arg("--max-balance")
        .arg("0.01")
        .arg("--min-target")
        .arg("0.02")
        .arg("--max-target")
        .arg("0.04")
        .arg("--min-hops")
        .arg("3")
        .arg("--max-hops")
        .arg("5")
        .arg("--min-delay-secs")
        .arg("15")
        .arg("--max-delay-secs")
        .arg("30")
        .arg("--min-gwei")
        .arg("1.2")
        .arg("--max-gwei")
        .arg("1.5")
        .arg("--workers")
        .arg("2")
        .arg("--min-worker-interval-secs")
        .arg("1")
        .arg("--max-worker-interval-secs")
        .arg("3")
        .arg("--load-concurrency")
        .arg("5");

    // This will fail at runtime because wallets/password/setup aren't real,
    // but should NOT fail with a clap argument-parsing error.
    cmd.assert().failure().stderr(
        predicate::str::contains("No wallets found")
            .or(predicate::str::contains("wallet dir"))
            .or(predicate::str::contains("WALLET_PASSWORD")),
    );
}
