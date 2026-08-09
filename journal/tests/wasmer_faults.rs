#![cfg(feature = "wasmer-evaluator")]

use journal_sdk::JOURNAL;

fn contained(expression: &str) {
    let result = JOURNAL.evaluate(expression);
    assert!(
        result.starts_with("(error '"),
        "fault unexpectedly completed: {result}"
    );
    assert_eq!(JOURNAL.evaluate("(+ 20 22)"), "42");
}

#[test]
fn wasm_kernel_contains_infinite_allocation_and_deep_result_faults() {
    contained("(let loop () (loop))");
    contained("(random-byte-vector 1000000000)");
    contained("(make-list 1000000000 #f)");
    let oversized_query = " ".repeat(17 * 1024 * 1024);
    assert_eq!(
        JOURNAL.evaluate(&oversized_query),
        "(error 'wasm-error \"kernel request exceeded\")"
    );
    assert_eq!(JOURNAL.evaluate("(+ 20 22)"), "42");
    let deep = JOURNAL
        .evaluate("(let loop ((i 0) (x '())) (if (= i 100000) x (loop (+ i 1) (cons i x))))");
    assert!(deep.starts_with("(error 'wasm-error") || deep.len() > 500_000);
    assert_eq!(JOURNAL.evaluate("(+ 20 22)"), "42");

    let record = "44".repeat(32);
    assert_eq!(
        JOURNAL.evaluate(&format!(
            "(sync-create (hex-string->byte-vector \"{record}\"))"
        )),
        "#t"
    );
    let oversized = JOURNAL.evaluate(&format!(
        "(sync-call '(make-list 2000000 #f) #t (hex-string->byte-vector \"{record}\"))"
    ));
    assert!(oversized.starts_with("(error '"), "{oversized}");
    assert!(oversized.len() < 4096, "oversized error escaped bound");
    assert_eq!(JOURNAL.evaluate("(+ 20 22)"), "42");
    assert_eq!(
        JOURNAL.evaluate(&format!(
            "(sync-delete (hex-string->byte-vector \"{record}\"))"
        )),
        "#t"
    );
}
