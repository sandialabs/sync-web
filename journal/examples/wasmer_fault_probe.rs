use journal_sdk::JOURNAL;
use std::time::Instant;
fn main() {
    for query in [
        "(random-byte-vector 1000000000)",
        "(make-list 1000000000)",
        "(let loop () (loop))",
        "(+ 1 2)",
    ] {
        let start = Instant::now();
        println!("{} {:?}", JOURNAL.evaluate(query), start.elapsed());
    }
}
