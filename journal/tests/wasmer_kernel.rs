#![cfg(feature = "wasmer-evaluator")]

use journal_sdk::JOURNAL;
use std::io::{Read, Write};
use std::net::TcpListener;

#[test]
fn oversized_blocking_call_fails_at_the_unchanged_capability_bound() {
    let symbol = "x".repeat(4_190_208);
    let result = JOURNAL.evaluate(&format!("(sync-call '({symbol}) #t)"));
    assert!(
        result.contains("bounded capability request exceeded"),
        "{result}"
    );
    assert_eq!(JOURNAL.evaluate("(+ 20 22)"), "42");
}

#[test]
fn wasm_kernel_basic_exact_and_next_health() {
    assert!(std::env::var_os("SYNC_WEB_WASMER_KERNEL").is_some());
    assert!(std::env::var_os("SYNC_WEB_WASMER_KERNEL_SHA256").is_some());
    assert_eq!(JOURNAL.evaluate("(+ 1 2)"), "3");
    assert_eq!(
        JOURNAL.evaluate("(catch #t (lambda () (error 'x \"y\")) (lambda args 'caught))"),
        "caught"
    );
    assert_eq!(
        JOURNAL.evaluate("(catch #t (lambda () (sync-car 1)) (lambda args 'callback-caught))"),
        "callback-caught"
    );
    assert_eq!(
        JOURNAL.evaluate("'(alpha (1 2) #u(3 4))"),
        "(alpha (1 2) #u(3 4))"
    );
    assert_eq!(JOURNAL.evaluate("(length (make-list 10000 1))"), "10000");
    assert_eq!(JOURNAL.evaluate("(+ 2 3)"), "5");
    for (name, expected) in [
        (
            "sync-create",
            "(sync-create id) create a new synchronic record with the given 32-byte ID",
        ),
        (
            "sync-delete",
            "(sync-delete id) delete the synchronic record with the given 32-byte ID",
        ),
        (
            "sync-all",
            "(sync-all) list all synchronic record IDs in ascending order",
        ),
        (
            "sync-call",
            "(sync-call query blocking? id) query the provided record ID or self if ID not provided",
        ),
        (
            "sync-http",
            "(sync-http method url . data) make an http request where method is 'get or 'post",
        ),
        (
            "sync-remote",
            "(sync-remote url data) make a post http request with the data payload)",
        ),
    ] {
        assert_eq!(
            JOURNAL.evaluate(&format!("(documentation {name})")),
            format!("\"{expected}\"")
        );
    }

    let ids = ["03".repeat(32), "01".repeat(32), "02".repeat(32)];
    for id in &ids {
        assert_eq!(
            JOURNAL.evaluate(&format!("(sync-create (hex-string->byte-vector \"{id}\"))")),
            "#t"
        );
    }
    assert_eq!(
        JOURNAL.evaluate("(map byte-vector->hex-string (sync-all))"),
        format!(
            "(\"{}\" \"{}\" \"{}\" \"{}\")",
            "00".repeat(32),
            "01".repeat(32),
            "02".repeat(32),
            "03".repeat(32)
        )
    );
    let root_delete = JOURNAL.evaluate(&format!(
        "(sync-delete (hex-string->byte-vector \"{}\"))",
        "00".repeat(32)
    ));
    assert!(root_delete.contains("cannot delete the root record"));
    for id in &ids {
        assert_eq!(
            JOURNAL.evaluate(&format!("(sync-delete (hex-string->byte-vector \"{id}\"))")),
            "#t"
        );
    }

    let mut server = mockito::Server::new();
    let response = server
        .mock("GET", "/uppercase")
        .with_status(200)
        .with_body("ok")
        .expect(1)
        .create();
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let result = runtime.block_on(async {
        tokio::task::block_in_place(|| {
            JOURNAL.evaluate(&format!("(sync-http 'GET \"{}/uppercase\")", server.url()))
        })
    });
    assert_eq!(result, "#u(111 107)");
    response.assert();

    let listener = TcpListener::bind("127.0.0.1:0").expect("oversized listener");
    let address = listener.local_addr().expect("listener address");
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("oversized connection");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                .expect("chunked header");
            let chunk = vec![b'x'; 1024 * 1024];
            for _ in 0..5 {
                if stream.write_all(b"100000\r\n").is_err()
                    || stream.write_all(&chunk).is_err()
                    || stream.write_all(b"\r\n").is_err()
                {
                    break;
                }
            }
            let _ = stream.write_all(b"0\r\n\r\n");
        }
    });
    for _ in 0..2 {
        let result = runtime.block_on(async {
            tokio::task::block_in_place(|| {
                JOURNAL.evaluate(&format!("(sync-http 'get \"http://{address}/oversized\")"))
            })
        });
        assert!(result.starts_with("(error 'sync-web-error"), "{result}");
        assert_eq!(JOURNAL.evaluate("(+ 20 22)"), "42");
    }
    server.join().expect("oversized server");
}
