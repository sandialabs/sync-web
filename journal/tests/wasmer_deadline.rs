#![cfg(feature = "wasmer-evaluator")]

use journal_sdk::JOURNAL;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

#[test]
#[ignore = "explicit 25-second Wasmer containment probe"]
fn blocking_http_obeys_one_top_level_deadline_and_recovers() {
    assert!(std::env::var_os("SYNC_WEB_WASMER_KERNEL").is_some());
    let listener = TcpListener::bind("127.0.0.1:0").expect("deadline listener");
    let address = listener.local_addr().expect("deadline address");
    let server = std::thread::spawn(move || {
        for delay in [3, 30] {
            let (mut stream, _) = listener.accept().expect("deadline connection");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            std::thread::sleep(Duration::from_secs(delay));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        }
    });
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let started = Instant::now();
    let result = runtime.block_on(async {
        tokio::task::block_in_place(|| {
            JOURNAL.evaluate(&format!(
                "(begin (sync-http 'get \"http://{address}/first\") (sync-http 'get \"http://{address}/slow\"))"
            ))
        })
    });
    let elapsed = started.elapsed();
    assert!(result.starts_with("(error '"), "{result}");
    assert!(
        elapsed >= Duration::from_secs(23),
        "deadline fired early: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(28),
        "deadline was reset: {elapsed:?}"
    );
    assert_eq!(JOURNAL.evaluate("(+ 20 22)"), "42");
    server.join().expect("deadline server");
}
