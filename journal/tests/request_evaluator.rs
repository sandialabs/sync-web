#![cfg(feature = "wasmer-evaluator")]

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn journal_sdk() -> Command {
    Command::new(env!("CARGO_BIN_EXE_journal-sdk"))
}

fn ledger() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ledger"))
}

fn temporary_database(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "sync-web-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ))
}

fn ledger_size(database: &std::path::Path) -> std::process::Output {
    ledger()
        .args([
            "--database",
            database.to_str().unwrap(),
            "--secret",
            "request-evaluator-test-root",
            "--evaluate",
            "((function size) (arguments ()))",
        ])
        .env("SYNC_WEB_INTERFACE_SECRET", "request-evaluator-test-interface")
        .output()
        .expect("ledger size")
}

fn assert_ledger_size(output: &std::process::Output, phase: &str) {
    assert!(
        output.status.success(),
        "{phase}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<i64>()
        .unwrap_or_else(|_| panic!("{phase}: unexpected Ledger output"));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("wasmer-runtime-attestation evaluator=wasmer-nofuel"),
        "{phase}: missing Wasmer attestation"
    );
}

#[test]
fn server_request_evaluator_fails_closed_without_verified_aot() {
    let missing_database = temporary_database("journal-missing-aot");
    let missing = journal_sdk()
        .args(["--database", missing_database.to_str().unwrap()])
        .arg("--evaluate")
        .arg("(+ 1 2)")
        .env_remove("SYNC_WEB_WASMER_KERNEL")
        .env_remove("SYNC_WEB_WASMER_KERNEL_SHA256")
        .output()
        .expect("journal-sdk without AOT");
    assert_eq!(missing.status.code(), Some(78));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("Journal request evaluator unavailable")
    );
    assert!(!missing_database.exists(), "failed health created Journal database");

    let invalid_database = temporary_database("journal-invalid-aot");
    let invalid = journal_sdk()
        .args(["--database", invalid_database.to_str().unwrap()])
        .arg("--evaluate")
        .arg("(+ 1 2)")
        .env("SYNC_WEB_WASMER_KERNEL", "/missing/kernel.wasmer")
        .env(
            "SYNC_WEB_WASMER_KERNEL_SHA256",
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .output()
        .expect("journal-sdk with invalid AOT");
    assert_eq!(invalid.status.code(), Some(78));
    assert!(!invalid_database.exists(), "invalid health created Journal database");
}

#[test]
fn evaluator_health_bypasses_installed_record_dispatch_on_reopen() {
    let database = temporary_database("installed-health");
    let code = "(lambda (*sync-state* query) (if (pair? query) (cons 'installed-ok *sync-state*) (error 'wrong-type-arg \"installed interface requires pair\")))";
    let bytes = code
        .bytes()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let install = format!(
        "(begin (set! *sync-state* (sync-cons #u({bytes}) (sync-cdr *sync-state*))) #t)"
    );
    let first = journal_sdk()
        .args(["--database", database.to_str().unwrap(), "--evaluate", &install])
        .output()
        .expect("install rejecting record");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&first.stdout).trim(), "#t");

    let reopen = journal_sdk()
        .args([
            "--database",
            database.to_str().unwrap(),
            "--evaluate",
            "(quote (health))",
        ])
        .output()
        .expect("reopen installed record");
    assert!(
        reopen.status.success(),
        "{}",
        String::from_utf8_lossy(&reopen.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&reopen.stdout).trim(), "installed-ok");
    assert!(
        String::from_utf8_lossy(&reopen.stderr)
            .contains("wasmer-runtime-attestation evaluator=wasmer-nofuel")
    );
    fs::remove_dir_all(database).expect("remove test database");
}

#[test]
fn ledger_installs_fresh_and_reopens_with_verified_aot() {
    let database = temporary_database("ledger-health");
    for phase in ["fresh", "reopen"] {
        assert_ledger_size(&ledger_size(&database), phase);
    }
    fs::remove_dir_all(database).expect("remove Ledger test database");
}

#[test]
fn ledger_fails_closed_without_verified_aot() {
    let missing_database = temporary_database("ledger-missing-aot");
    let missing = ledger()
        .args(["--database", missing_database.to_str().unwrap(), "--evaluate", "#t"])
        .env_remove("SYNC_WEB_WASMER_KERNEL")
        .env_remove("SYNC_WEB_WASMER_KERNEL_SHA256")
        .output()
        .expect("ledger without AOT");
    assert_eq!(missing.status.code(), Some(78));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("Journal request evaluator unavailable")
    );
    assert!(!missing_database.exists(), "failed health created Ledger database");
    assert_ledger_size(
        &ledger_size(&missing_database),
        "same-database recovery after missing AOT",
    );
    fs::remove_dir_all(&missing_database).expect("remove recovered Ledger database");

    let invalid_database = temporary_database("ledger-invalid-aot");
    let invalid = ledger()
        .args(["--database", invalid_database.to_str().unwrap(), "--evaluate", "#t"])
        .env("SYNC_WEB_WASMER_KERNEL", "/missing/kernel.wasmer")
        .env(
            "SYNC_WEB_WASMER_KERNEL_SHA256",
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .output()
        .expect("ledger with invalid AOT");
    assert_eq!(invalid.status.code(), Some(78));
    assert!(!invalid_database.exists(), "invalid health created Ledger database");
    assert_ledger_size(
        &ledger_size(&invalid_database),
        "same-database recovery after invalid AOT",
    );
    fs::remove_dir_all(invalid_database).expect("remove recovered Ledger database");
}

#[test]
fn invalid_leaf_limit_cannot_bypass_required_aot_health() {
    for (name, mut command) in [("journal", journal_sdk()), ("ledger", ledger())] {
        let database = temporary_database(&format!("{name}-leaf-bypass"));
        let output = command
            .args(["--database", database.to_str().unwrap(), "--evaluate", "#t"])
            .env_remove("SYNC_WEB_WASMER_KERNEL")
            .env_remove("SYNC_WEB_WASMER_KERNEL_SHA256")
            .env("SYNC_WEB_MAX_LEAF_BYTES", "not-a-number")
            .output()
            .expect("invalid leaf limit with missing AOT");
        assert_eq!(output.status.code(), Some(78), "{name}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("Journal request evaluator unavailable"),
            "{name}"
        );
        assert!(!database.exists(), "{name}: failed health created database");
    }
}

#[test]
fn legacy_evaluator_selector_cannot_select_native_requests() {
    let output = journal_sdk()
        .arg("--evaluate")
        .arg("(+ 1 2)")
        .env("SYNC_WEB_EVALUATOR", "native")
        .output()
        .expect("journal-sdk with ignored legacy selector");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "3");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("wasmer-runtime-attestation evaluator=wasmer-nofuel")
    );
}
