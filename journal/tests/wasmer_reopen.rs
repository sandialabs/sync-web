#![cfg(feature = "wasmer-evaluator")]

use journal_sdk::JOURNAL;
use std::process::Command;

fn child(
    phase: &str,
    database: &std::path::Path,
    state_path: &std::path::Path,
) -> std::process::Output {
    Command::new(std::env::current_exe().expect("test executable"))
        .env("SYNC_WEB_WASMER_REOPEN_PHASE", phase)
        .env("SYNC_WEB_DATABASE", database)
        .env("SYNC_WEB_WASMER_STATE_PATH", state_path)
        .output()
        .expect("reopen child")
}

#[test]
fn wasm_kernel_reopens_exact_persisted_state() {
    match std::env::var("SYNC_WEB_WASMER_REOPEN_PHASE").as_deref() {
        Ok("write") => {
            assert_eq!(
                JOURNAL.evaluate(
                    "(begin (set! *sync-state* (sync-cons (sync-car (sync-state)) #u(77))) #t)"
                ),
                "#t"
            );
            assert_eq!(JOURNAL.evaluate("(sync-cdr (sync-state))"), "#u(77)");
            std::fs::write(
                std::env::var("SYNC_WEB_WASMER_STATE_PATH").expect("state path"),
                JOURNAL.evaluate("(sync-state)"),
            )
            .expect("write physical state word");
        }
        Ok("read") => assert_eq!(JOURNAL.evaluate("(sync-cdr (sync-state))"), "#u(77)"),
        _ => {
            assert!(std::env::var_os("SYNC_WEB_WASMER_KERNEL").is_some());
            assert!(std::env::var_os("SYNC_WEB_WASMER_KERNEL_SHA256").is_some());
            let root = std::env::temp_dir().join(format!(
                "sync-web-wasmer-reopen-{}-{}",
                std::process::id(),
                rand::random::<u64>()
            ));
            let database = root.join("database");
            let state = root.join("state");
            std::fs::create_dir_all(&root).expect("reopen root");
            for phase in ["write", "read"] {
                let output = child(phase, &database, &state);
                assert!(
                    output.status.success(),
                    "{phase} failed: stdout={} stderr={}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            assert!(!std::fs::read(&state).expect("physical state").is_empty());
            std::fs::remove_dir_all(root).expect("remove reopen database");
        }
    }
}
