use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn wallet_balance_dump_help_works() {
    let mut cmd = Command::cargo_bin("wallet-balance-dump").unwrap();

    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("wallet-balance-dump"))
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--output"));
}
