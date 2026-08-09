use hex;
use journal_sdk::{JOURNAL, Word};
use mockito;
use rand::RngCore;

pub fn setup() -> (String, impl Fn(&str, &str)) {
    let mut seed: Word = [0 as u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    let record = hex::encode(seed);

    assert!(
        JOURNAL
            .evaluate(format!("(sync-create (hex-string->byte-vector \"{}\"))", record,).as_str())
            == "#t",
        "Unable to set up new Journal",
    );

    (record.clone(), move |expression, expected| {
        let result = JOURNAL.evaluate(
            format!(
                "(sync-call '{} #t (hex-string->byte-vector \"{}\")))",
                expression, record,
            )
            .as_str(),
        );
        assert!(
            result == String::from(expected),
            "Assertion failed: {} --> {} not {}",
            expression,
            result,
            expected,
        );
    })
}

#[test]
fn test_self() {
    let (_record, assert1) = setup();
    assert1("(sync-call '(+ 2 2) #t)", "4");
}

#[test]
fn test_record() {
    let (_record1, assert1) = setup();
    let (record2, _assert2) = setup();

    assert1(
        format!(
            "(sync-call '(+ 2 2) #t (hex-string->byte-vector \"{}\"))",
            record2
        )
        .as_str(),
        "4",
    );
    assert1(
        format!(
            "(sync-call ''(+ 2 2) #t (hex-string->byte-vector \"{}\"))",
            record2
        )
        .as_str(),
        "(+ 2 2)",
    );
    assert1(
        format!(
            "(sync-call '{} #t (hex-string->byte-vector \"{}\"))",
            "(begin (set! *sync-state* (sync-cons (sync-car *sync-state*) #u(2))) #t)", record2,
        )
        .as_str(),
        "#t",
    );
}

#[test]
fn test_remote_status_failure_is_not_cached() {
    let mut server = mockito::Server::new();
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(async move {
        let failure = server
            .mock("POST", "/remote")
            .with_status(503)
            .with_body("#t")
            .expect(1)
            .create();
        let expression = format!("(sync-remote \"{}/remote\" '(request))", server.url());
        assert_eq!(
            JOURNAL.evaluate(&expression),
            "(error 'sync-web-error \"Journal is unable to query remote peer (sync-remote)\" ((data (\"Journal is unable to query remote peer (sync-remote)\"))))"
        );
        failure.assert();
        drop(failure);

        let success = server
            .mock("POST", "/remote")
            .with_status(200)
            .with_body("#t")
            .expect(1)
            .create();
        assert_eq!(JOURNAL.evaluate(&expression), "#t");
        success.assert();

        let error = "(error 'sync-web-error \"Journal is unable to query remote peer (sync-remote)\" ((data (\"Journal is unable to query remote peer (sync-remote)\"))))";
        let invalid_responses = [
            ("/empty", 204, Vec::new()),
            ("/nul", 200, b"#t\0#f".to_vec()),
            ("/malformed", 200, b"(".to_vec()),
            ("/trailing", 200, b"#t #f".to_vec()),
            ("/encoding", 200, vec![0xff]),
        ];
        for (path, status, body) in invalid_responses {
            let invalid = server
                .mock("POST", path)
                .with_status(status)
                .with_body(body)
                .expect(1)
                .create();
            let expression = format!("(sync-remote \"{}{}\" '(request))", server.url(), path);
            assert_eq!(JOURNAL.evaluate(&expression), error);
            assert_eq!(JOURNAL.evaluate("(+ 1 2)"), "3");
            invalid.assert();
            drop(invalid);

            let valid = server
                .mock("POST", path)
                .with_status(200)
                .with_body("#t")
                .expect(1)
                .create();
            assert_eq!(JOURNAL.evaluate(&expression), "#t");
            valid.assert();
        }

        let (record, _) = setup();
        let before = JOURNAL.evaluate(&format!(
            "(sync-call '(sync-digest *sync-state*) #t (hex-string->byte-vector \"{}\"))",
            record
        ));
        let failure = server
            .mock("POST", "/atomic")
            .with_status(503)
            .with_body("#t")
            .expect(1)
            .create();
        let result = JOURNAL.evaluate(&format!(
            "(sync-call '(begin (set! *sync-state* (sync-cons *sync-state* #u(1))) (sync-remote \"{}/atomic\" '(request))) #t (hex-string->byte-vector \"{}\"))",
            server.url(), record
        ));
        assert!(result.contains("Journal is unable to query remote peer (sync-remote)"));
        failure.assert();
        assert_eq!(
            JOURNAL.evaluate(&format!(
                "(sync-call '(sync-digest *sync-state*) #t (hex-string->byte-vector \"{}\"))",
                record
            )),
            before
        );
    });
}

#[test]
fn test_http_status_failure_is_not_cached() {
    let mut server = mockito::Server::new();
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(async move {
        let failure = server
            .mock("GET", "/http")
            .with_status(429)
            .with_body("forged success")
            .expect(1)
            .create();
        let expression = format!("(sync-http 'get \"{}/http\")", server.url());
        assert_eq!(
            JOURNAL.evaluate(&expression),
            "(error 'sync-web-error \"Journal is unable to fulfill HTTP request (sync-http)\" ((data (\"Journal is unable to fulfill HTTP request (sync-http)\"))))"
        );
        failure.assert();
        drop(failure);

        let success = server
            .mock("GET", "/http")
            .with_status(201)
            .with_body("ok")
            .expect(1)
            .create();
        assert_eq!(
            JOURNAL.evaluate(&format!(
                "(byte-vector->string (sync-http 'get \"{}/http\"))",
                server.url()
            )),
            "\"ok\""
        );
        success.assert();

        let redirect = server
            .mock("GET", "/redirect")
            .with_status(302)
            .with_header("location", "/target")
            .expect(1)
            .create();
        let target = server
            .mock("GET", "/target")
            .with_status(200)
            .with_body("must not be requested")
            .expect(0)
            .create();
        assert!(JOURNAL
            .evaluate(&format!("(sync-http 'get \"{}/redirect\")", server.url()))
            .contains("Journal is unable to fulfill HTTP request (sync-http)"));
        redirect.assert();
        target.assert();
    });
}

#[test]
fn test_http() {
    let (_record, assert) = setup();

    let mut server = mockito::Server::new();
    let url = server.url();

    tokio::task::block_in_place(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                server
                    .mock("GET", "/hello")
                    .with_status(200)
                    .with_header("content-type", "text/plain")
                    .with_body("hello, world!")
                    .create();

                server
                    .mock("POST", "/hello")
                    .match_body(mockito::Matcher::Exact("world".to_string()))
                    .with_status(200)
                    .with_header("content-type", "text/plain")
                    .with_body("greeted")
                    .create();

                assert(
                    format!("(byte-vector->string (sync-http 'get \"{}/hello\"))", url,).as_str(),
                    "\"hello, world!\"",
                );

                assert(
                    format!(
                        "(byte-vector->string (sync-http 'post \"{}/hello\" \"world\"))",
                        url,
                    )
                    .as_str(),
                    "\"greeted\"",
                );
            })
    })
}
