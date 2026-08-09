#![cfg(not(feature = "wasm-kernel"))]

use journal_sdk::JOURNAL;
use std::process::Command;

fn child(phase: &str, maximum: &str) -> std::process::Output {
    Command::new(std::env::current_exe().expect("test executable"))
        .env("SYNC_WEB_LEAF_LIMIT_PHASE", phase)
        .env("SYNC_WEB_MAX_LEAF_BYTES", maximum)
        .output()
        .expect("leaf-limit child")
}

#[test]
#[ignore = "explicit maximum-plus-one malicious operation probe"]
fn maximum_plus_one_leaf_operation_is_contained() {
    let result = JOURNAL.evaluate(
        "(begin (set! *sync-state* (sync-cons (sync-car (sync-state)) (make-byte-vector 67108865 1))) #t)",
    );
    assert!(result.starts_with("(error '"), "{result}");
    assert_eq!(JOURNAL.evaluate("(+ 20 22)"), "42");
}

#[test]
fn configured_leaf_limit_fails_closed_and_covers_native_and_wasmer() {
    match std::env::var("SYNC_WEB_LEAF_LIMIT_PHASE").as_deref() {
        Ok("invalid") => assert_eq!(
            JOURNAL.evaluate("(+ 1 2)"),
            "(error 'configuration-error \"SYNC_WEB_MAX_LEAF_BYTES must be a decimal byte count\")"
        ),
        Ok("invalid-range") => assert_eq!(
            JOURNAL.evaluate("(+ 1 2)"),
            "(error 'configuration-error \"SYNC_WEB_MAX_LEAF_BYTES must be between 62 and 67108864\")"
        ),
        Ok("health") => assert_eq!(JOURNAL.evaluate("(+ 1 2)"), "3"),
        Ok("boundary") => {
            let result = JOURNAL.evaluate(
                "(begin (set! *sync-state* (sync-cons (sync-car (sync-state)) (make-byte-vector 256 1))) #t)",
            );
            assert_eq!(result, "#t");
            assert_eq!(JOURNAL.evaluate("(+ 20 22)"), "42");
        }
        Ok("oversized") => {
            let result = JOURNAL.evaluate(
                "(begin (set! *sync-state* (sync-cons (sync-car (sync-state)) (make-byte-vector 257 1))) #t)",
            );
            assert!(result.starts_with("(error '"), "{result}");
            assert_eq!(JOURNAL.evaluate("(+ 20 22)"), "42");
        }
        _ => {
            for (phase, maximum) in [
                ("invalid", "invalid"),
                ("invalid-range", "0"),
                ("invalid-range", "61"),
                ("invalid-range", "67108865"),
                ("health", "62"),
                ("boundary", "256"),
                ("oversized", "256"),
            ] {
                let output = child(phase, maximum);
                assert!(
                    output.status.success(),
                    "Wasmer {phase} failed: stdout={} stderr={}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}
