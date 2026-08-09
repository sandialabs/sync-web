use journal_sdk::JOURNAL;
use std::time::{Duration, Instant};

fn main() {
    let n: usize = std::env::var("N")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100);
    let started = Instant::now();
    for _ in 0..n {
        assert_eq!(JOURNAL.evaluate("(+ 1 2)"), "3");
    }
    eprintln!(
        "READY pid={} evaluations={} elapsed-ns={}",
        std::process::id(),
        n,
        started.elapsed().as_nanos()
    );
    std::thread::sleep(Duration::from_secs(
        std::env::var("SLEEP_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(30),
    ));
}
