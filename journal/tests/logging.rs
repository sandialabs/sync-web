use journal_sdk::JOURNAL;
use log::{LevelFilter, Log, Metadata, Record};
use std::sync::{Mutex, Once};

struct CaptureLogger(Mutex<Vec<String>>);

impl Log for CaptureLogger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        self.0
            .lock()
            .expect("capture logger lock poisoned")
            .push(format!("{}", record.args()));
    }

    fn flush(&self) {}
}

static LOGGER: CaptureLogger = CaptureLogger(Mutex::new(Vec::new()));
static INIT: Once = Once::new();

#[test]
fn evaluation_diagnostics_omit_request_and_result_bodies() {
    INIT.call_once(|| {
        log::set_logger(&LOGGER).expect("failed to install capture logger");
        log::set_max_level(LevelFilter::Debug);
    });
    LOGGER.0.lock().expect("capture logger lock poisoned").clear();

    let sentinel = "JOURNAL-LOG-LEAK-SENTINEL";
    let result = JOURNAL.evaluate(&format!("(error 'probe \"{sentinel}\")"));
    assert!(result.contains(sentinel));
    JOURNAL.scheme_to_json(&format!("(malformed . {sentinel}"));

    let logged = LOGGER
        .0
        .lock()
        .expect("capture logger lock poisoned")
        .join("\n");
    assert!(!logged.contains(sentinel), "logged sensitive body: {logged}");
}
