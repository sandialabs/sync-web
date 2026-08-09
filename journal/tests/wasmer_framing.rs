#![cfg(feature = "wasmer-evaluator")]

use journal_sdk::{JOURNAL, Word};
use rand::RngCore;

fn validate_evaluator_artifact() {
    assert!(std::env::var_os("SYNC_WEB_WASMER_KERNEL").is_some());
    assert!(std::env::var_os("SYNC_WEB_WASMER_KERNEL_SHA256").is_some());
}

fn create_record() -> String {
    let mut record: Word = [0; 32];
    rand::thread_rng().fill_bytes(&mut record);
    let record = hex::encode(record);
    assert_eq!(
        JOURNAL.evaluate(&format!(
            "(sync-create (hex-string->byte-vector \"{record}\"))"
        )),
        "#t"
    );
    record
}

fn call(record: &str, expression: &str) -> String {
    JOURNAL.evaluate(&format!(
        "(sync-call '{expression} #t (hex-string->byte-vector \"{record}\"))"
    ))
}

fn mutation_query(result_bytes: usize) -> String {
    format!(
        "(let ((result (make-byte-vector {result_bytes} 1))) (byte-vector-set! result (- {result_bytes} 1) 2) (set! *sync-state* (sync-cons (sync-car *sync-state*) (sync-cons (sync-cdr *sync-state*) #u(1)))) result)"
    )
}

fn mutation_count(record: &str) -> String {
    call(
        record,
        "(let loop ((node (sync-cdr *sync-state*)) (n 0)) (if (and (sync-pair? node) (equal? (sync-cdr node) #u(1))) (loop (sync-car node) (+ n 1)) n))",
    )
}

#[test]
fn oversized_results_do_not_replay_effects() {
    validate_evaluator_artifact();

    let small = create_record();
    let small_result = call(&small, &mutation_query(1000));
    assert!(small_result.len() < 4096, "{small_result}");
    assert_eq!(mutation_count(&small), "1");

    let large = create_record();
    let large_query = mutation_query(5000);
    let first_large = call(&large, &large_query);
    assert!(first_large.len() > 4096, "{first_large}");
    assert_eq!(mutation_count(&large), "1");
    let second_large = call(&large, &large_query);
    assert!(second_large.len() > 4096, "{second_large}");
    assert_eq!(
        mutation_count(&large),
        "2",
        "two intentional identical calls must each execute exactly once"
    );

    let nested_target = create_record();
    let nested_caller = create_record();
    let nested = format!(
        "(sync-call '{} #t (hex-string->byte-vector \"{}\"))",
        mutation_query(5000),
        nested_target
    );
    assert!(call(&nested_caller, &nested).len() > 4096);
    assert_eq!(mutation_count(&nested_target), "1");

    let error_effect = create_record();
    let error_source = create_record();
    let error_query = format!(
        "(begin (sync-call '(begin (set! *sync-state* (sync-cons (sync-car *sync-state*) (sync-cons (sync-cdr *sync-state*) #u(1)))) #t) #t (hex-string->byte-vector \"{error_effect}\")) (let ((detail (make-byte-vector 5000 1))) (byte-vector-set! detail 4999 2) (error 'framing detail)))"
    );
    let error = call(&error_source, &error_query);
    assert!(error.starts_with("(error 'framing"), "{error}");
    assert!(error.len() > 4096);
    assert_eq!(mutation_count(&error_effect), "1");
    assert_eq!(call(&error_source, "(+ 20 22)"), "42");
    assert!(call(&error_source, &error_query).len() > 4096);
    assert_eq!(
        mutation_count(&error_effect),
        "2",
        "an earlier framing response must not survive into a later invocation"
    );

    let large_leaf = create_record();
    assert_eq!(
        call(
            &large_leaf,
            "(begin (set! *sync-state* (sync-cons (sync-car *sync-state*) (make-byte-vector 5000 7))) #t)"
        ),
        "#t"
    );
    assert_eq!(
        call(&large_leaf, "(length (sync-cdr *sync-state*))"),
        "5000"
    );
    assert_eq!(
        call(
            &large_leaf,
            "(byte-vector-ref (sync-cdr *sync-state*) 4999)"
        ),
        "7"
    );

    let mut server = mockito::Server::new();
    let response = server
        .mock("GET", "/large")
        .with_status(200)
        .with_body(format!("{}y", "x".repeat(4999)))
        .expect(1)
        .create();
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let http = runtime.block_on(async {
        tokio::task::block_in_place(|| {
            JOURNAL.evaluate(&format!("(sync-http 'get \"{}/large\")", server.url()))
        })
    });
    assert!(http.len() > 4096);
    response.assert();

    let mut roots_query = String::from("(begin ");
    for _ in 0..48 {
        let mut record: Word = [0; 32];
        rand::thread_rng().fill_bytes(&mut record);
        roots_query.push_str(&format!(
            "(sync-create (hex-string->byte-vector \"{}\")) ",
            hex::encode(record)
        ));
    }
    roots_query.push_str("(sync-all))");
    let roots = JOURNAL.evaluate(&roots_query);
    assert!(roots.len() > 4096, "large root response: {}", roots.len());
    assert_eq!(JOURNAL.evaluate("(+ 20 22)"), "42");
}
