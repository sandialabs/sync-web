#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

#[cfg(all(not(feature = "wasm-kernel"), not(feature = "wasmer-evaluator")))]
compile_error!("journal-sdk request evaluation requires the wasmer-evaluator feature");
#[cfg(all(
    not(feature = "wasm-kernel"),
    feature = "wasmer-evaluator",
    not(all(target_os = "linux", target_arch = "x86_64"))
))]
compile_error!("journal-sdk Wasmer request evaluation is qualified only for Linux x86_64");

use crate::cache::{
    ResolveSource, ResolvedNode, resolve_branch_with, resolve_node_with, resolve_stump_with,
};
pub use crate::config::Config;
use crate::evaluator::{Primitive, Type, json2lisp, lisp2json, obj2str};
use crate::persistor::{MemoryPersistor, PERSISTOR, Persistor, maximum_leaf_bytes};
pub use crate::persistor::{SIZE, Word};
use crate::scenario_context::ScenarioContext;
use crate::serialization::{
    serialization_trace_active, serialization_trace_child, serialization_trace_pair,
};
use libc;
use log::{debug, info, warn};
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use evaluator as s7;
use sha2::{Digest, Sha256};
use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_char;
#[cfg(target_env = "msvc")]
use std::os::raw::c_uchar;

mod cache;
mod config;
pub mod evaluator;
mod persistor;
mod primitives;
mod scenario_context;
mod serialization;
#[cfg(feature = "test-support")]
pub mod test_support;
#[cfg(feature = "wasmer-evaluator")]
mod wasmer_runtime;

use primitives::*;
mod extensions {
    pub mod crypto;
    pub mod system;
}

#[cfg(not(feature = "wasm-kernel"))]
pub static JOURNAL: Lazy<Journal> = Lazy::new(|| Journal::new());

pub(crate) const SYNC_NODE_TAG: i64 = 0;

const GENESIS_STR: &str = "(lambda (*sync-state* query) (cons (eval query) *sync-state*))";

pub(crate) const NULL: Word = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

pub(crate) struct Session {
    pub(crate) record: Word,
    pub(crate) state: Word,
    pub(crate) persistor: MemoryPersistor,
    pub(crate) cache: Arc<Mutex<HashMap<(String, String, Vec<u8>), Vec<u8>>>>,
    pub(crate) serialization_query_locs: HashMap<String, s7::s7_int>,
    pub(crate) sync_eval_header_cache: HashMap<Word, Vec<u8>>,
    pub(crate) sync_let_base_env_loc: Option<s7::s7_int>,
    pub(crate) sync_let_boundaries: Vec<s7::s7_int>,
    pub(crate) external_called: bool,
    pub(crate) scenario: Option<ScenarioContext>,
}

impl Session {
    fn new(
        record: Word,
        state: Word,
        persistor: MemoryPersistor,
        cache: Arc<Mutex<HashMap<(String, String, Vec<u8>), Vec<u8>>>>,
    ) -> Self {
        Self {
            record,
            state,
            persistor,
            cache,
            serialization_query_locs: HashMap::new(),
            sync_eval_header_cache: HashMap::new(),
            sync_let_base_env_loc: None,
            sync_let_boundaries: Vec::new(),
            external_called: false,
            scenario: None,
        }
    }
}

pub(crate) static SESSIONS: Lazy<RwLock<HashMap<usize, Session>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub(crate) fn scenario_random_bytes(sc: *mut s7::s7_scheme, length: usize) -> Option<Vec<u8>> {
    SESSIONS
        .read()
        .expect("Failed to acquire sessions lock")
        .get(&(sc as usize))
        .and_then(|session| session.scenario.as_ref())
        .map(|scenario| scenario.random_bytes(length))
}

pub(crate) fn scenario_unix_time(sc: *mut s7::s7_scheme) -> Option<i64> {
    SESSIONS
        .read()
        .expect("Failed to acquire sessions lock")
        .get(&(sc as usize))
        .and_then(|session| session.scenario.as_ref())
        .map(ScenarioContext::unix_time)
}

struct CallOnDrop<F: FnMut()>(F);

impl<F: FnMut()> Drop for CallOnDrop<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

#[derive(Debug)]
pub struct JournalAccessError(pub Word);

#[cfg(not(feature = "wasm-kernel"))]
static LOCK: Mutex<()> = Mutex::new(());
#[cfg(not(feature = "wasm-kernel"))]
static RUNS: usize = 1;

fn escape_scheme_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\"', "\\\"")
}

fn warn_on_error_result(output: &str) {
    if output.starts_with("(error ") {
        warn!("Evaluation returned error form; request and result omitted");
    }
}

/// Journals are the primary way that application developers
/// interact with the synchronic web.
///
/// Conceptually, a Journal is a
/// service that interacts with users and other Journals (nodes) to
/// persist synchronic web state. Behind the schemes, it is
/// responsible for two capabilities:
///
/// * __Persistence__: managing bytes on the global hash graph
///
/// * __Evaluation__: executing code in the global Scheme environment
///
/// __Records__ are the primary way that developers interface with
/// Journals. A Record is a mapping between a constant identifier and
/// mutable state. Both identifiers and state are represented as
/// fixed-size __Words__ that the outputs of a cryptographic hash
/// function. When a new record is created, the Journal returns a
/// record secret that is the second hash preimage of the identifier.
/// This is intended to be used so that applications can bootstrap
/// records into increasingly sophisticated notions of identity.
#[cfg(not(feature = "wasm-kernel"))]
const HTTP_TIMEOUT: Duration = Duration::from_secs(25);

#[cfg(not(feature = "wasm-kernel"))]
fn http_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to create remote HTTP client")
}

#[cfg(not(feature = "wasm-kernel"))]
async fn response_bytes(
    response: Result<reqwest::Response, reqwest::Error>,
) -> Result<Vec<u8>, String> {
    let response = response.map_err(|_| "HTTP request failed".to_string())?;
    if !response.status().is_success() {
        return Err("HTTP status was not successful".to_string());
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|_| "HTTP response failed".to_string())
}

#[cfg(not(feature = "wasm-kernel"))]
async fn remote_post(client: &reqwest::Client, url: &str, body: String) -> Result<Vec<u8>, String> {
    response_bytes(client.post(url).body(body).send().await).await
}

#[cfg(not(feature = "wasm-kernel"))]
unsafe fn valid_remote_response(sc: *mut s7::s7_scheme, bytes: &[u8]) -> bool {
    unsafe {
        let Ok(body) = std::str::from_utf8(bytes) else {
            return false;
        };
        if body.trim().is_empty() || body.as_bytes().contains(&0) {
            return false;
        }
        let escaped = body.replace('\\', "\\\\").replace('"', "\\\"");
        let Ok(validator) = CString::new(format!(
            "(catch #t (lambda () (let* ((port (open-input-string \"{}\")) (value (read port)) (tail (read port))) (and (not (eof-object? value)) (eof-object? tail)))) (lambda args #f))",
            escaped
        )) else {
            return false;
        };
        let result = s7::s7_eval_c_string(sc, validator.as_ptr());
        s7::s7_is_boolean(result) && s7::s7_boolean(sc, result)
    }
}

#[cfg(not(feature = "wasm-kernel"))]
pub struct Journal {
    client: reqwest::Client,
}

#[cfg(not(feature = "wasm-kernel"))]
impl Journal {
    fn new() -> Self {
        let journal = || Self {
            client: http_client(HTTP_TIMEOUT),
        };
        #[cfg(not(feature = "wasm-kernel"))]
        if let Err(error) = wasmer_runtime::health_check() {
            eprintln!("Journal request evaluator unavailable: {error}");
            unsafe { libc::_exit(78) }
        }
        if maximum_leaf_bytes().is_err() {
            return journal();
        }
        let _ = PERSISTOR.root_new(
            NULL,
            PERSISTOR
                .branch_set(
                    PERSISTOR
                        .leaf_set(GENESIS_STR.as_bytes().to_vec())
                        .expect("Failed to create genesis leaf"),
                    NULL,
                    NULL,
                )
                .expect("Failed to create genesis branch"),
        );
        journal()
    }

    /// Evaluate a Scheme expression within a Record
    ///
    /// # Examples
    /// ```
    /// use journal_sdk::JOURNAL;
    ///
    /// // Simple expression
    /// let output = JOURNAL.evaluate("(+ 1 2)");
    /// assert!(output == "3");
    ///
    /// // Complex expression
    /// let output = JOURNAL.evaluate(
    ///     "(begin (define (add2 x) (+ x 2)) (add2 1))",
    /// );
    /// assert!(output == "3");
    pub fn evaluate(&self, query: &str) -> String {
        self.evaluate_record(NULL, query)
    }

    pub fn evaluate_json(&self, query: Value) -> Value {
        match json2lisp(&query) {
            Ok(scheme_query) => {
                let result = self.evaluate_record(NULL, scheme_query.as_str());
                match lisp2json(result.as_str()) {
                    Ok(json_result) => json_result,
                    Err(_) => {
                        log::warn!("Failed to parse Scheme result to JSON; result omitted");
                        lisp2json("(error 'parse-error \"Failed to parse Scheme to JSON\")")
                    }
                    .expect("Error parsing the JSON error message"),
                }
            }
            Err(_) => {
                log::warn!("Failed to parse JSON request to Scheme; request omitted");
                lisp2json("(error 'parse-error \"Failed to parse JSON to Scheme\")")
            }
            .expect("Error parsing the JSON error message"),
        }
    }

    /// Convert a Scheme expression into its JSON representation without evaluation.
    ///
    /// # Examples
    /// ```
    /// use journal_sdk::JOURNAL;
    /// use serde_json::json;
    ///
    /// let output = JOURNAL.scheme_to_json("(+ 1 2)");
    /// assert_eq!(output, json!(["+", 1, 2]));
    /// ```
    pub fn scheme_to_json(&self, query: &str) -> Value {
        match lisp2json(query) {
            Ok(json_result) => json_result,
            Err(_) => {
                log::warn!("Failed to parse Scheme request to JSON; request omitted");
                lisp2json("(error 'parse-error \"Failed to parse Scheme to JSON\")")
            }
            .expect("Error parsing the JSON error message"),
        }
    }

    /// Convert a JSON expression into its Scheme representation without evaluation.
    ///
    /// # Examples
    /// ```
    /// use journal_sdk::JOURNAL;
    /// use serde_json::json;
    ///
    /// let output = JOURNAL.json_to_scheme(json!(["+", 1, 2]));
    /// assert_eq!(output, "(+ 1 2)");
    /// ```
    pub fn json_to_scheme(&self, query: Value) -> String {
        match json2lisp(&query) {
            Ok(scheme_result) => scheme_result,
            Err(_) => {
                log::warn!("Failed to parse JSON request to Scheme; request omitted");
                "(error 'parse-error \"Failed to parse JSON to Scheme\")".to_string()
            }
        }
    }

    fn evaluate_record(&self, record: Word, query: &str) -> String {
        self.evaluate_record_with_context(record, query, None)
    }

    pub(crate) fn evaluate_record_with_context(
        &self,
        record: Word,
        query: &str,
        scenario: Option<ScenarioContext>,
    ) -> String {
        if let Err(error) = maximum_leaf_bytes() {
            return format!(
                "(error 'configuration-error \"{}\")",
                error.0.replace('"', "'")
            );
        }
        #[cfg(not(feature = "wasm-kernel"))]
        {
            wasmer_runtime::evaluate_record(self, record, query, scenario)
        }
        #[cfg(feature = "wasm-kernel")]
        {
            self.evaluate_record_native_with_context(record, query, scenario)
        }
    }

    fn evaluate_record_native_with_context(
        &self,
        record: Word,
        query: &str,
        scenario: Option<ScenarioContext>,
    ) -> String {
        let mut runs = 0;
        let cache = Arc::new(Mutex::new(HashMap::new()));

        let start = Instant::now();
        debug!("Evaluating request; body omitted");

        loop {
            let _lock1 = if runs >= RUNS {
                Some(LOCK.lock().expect("Failed to acquire concurrency lock"))
            } else {
                None
            };

            let (state_old, record_temp) = {
                let _lock2 = match _lock1 {
                    Some(_) => None,
                    None => Some(LOCK.lock().expect("Failed to acquire secondary lock")),
                };
                let state_old = PERSISTOR
                    .root_get(record)
                    .expect("Failed to get current state");
                let record_temp = PERSISTOR
                    .root_temp(state_old)
                    .expect("Failed to create temporary record");
                (state_old, record_temp)
            };

            let _record_dropper = CallOnDrop(|| {
                PERSISTOR
                    .root_delete(record_temp)
                    .expect("Failed to delete temporary record");
            });

            let genesis_branch = PERSISTOR
                .branch_get(state_old)
                .expect("Failed to get genesis branch");

            let genesis_func = PERSISTOR
                .leaf_get(genesis_branch.0)
                .expect("Failed to get genesis function")
                .to_vec();

            let genesis_str = String::from_utf8_lossy(&genesis_func);

            let evaluator = journal_evaluator();
            install_sync_let_runtime(&evaluator);

            let persistor_initial = MemoryPersistor::new();

            match PERSISTOR.branch_get(state_old) {
                Ok((left, right, digest)) => persistor_initial
                    .branch_set(left, right, digest)
                    .expect("Could not set state root branch to session persistor"),
                Err(_) => panic!("Could not set state root branch to session persistor"),
            };

            SESSIONS
                .write()
                .expect("Failed to acquire sessions lock")
                .insert(evaluator.sc as usize, {
                    let mut session =
                        Session::new(record, state_old, persistor_initial, cache.clone());
                    session.scenario = scenario.clone();
                    session
                });

            let _session_dropper = CallOnDrop(|| {
                let removed = SESSIONS
                    .write()
                    .expect("Failed to acquire sessions lock for cleanup")
                    .remove(&(evaluator.sc as usize));
                if let Some(session) = removed {
                    unsafe {
                        if let Some(location) = session.sync_let_base_env_loc {
                            s7::s7_gc_unprotect_at(evaluator.sc, location);
                        }
                        for location in session.serialization_query_locs.into_values() {
                            s7::s7_gc_unprotect_at(evaluator.sc, location);
                        }
                    }
                }
            });

            let expr = format!(
                "((eval {}) (sync-state) (read (open-input-string \"{}\")))",
                genesis_str,
                escape_scheme_string(query),
            );

            let result = evaluator.evaluate(expr.as_str());
            runs += 1;

            let (persistor, external_called) = {
                let session = SESSIONS.read().expect("Failed to acquire sessions lock");
                let session = session
                    .get(&(evaluator.sc as usize))
                    .expect("Session not found in SESSIONS map");
                (session.persistor.clone(), session.external_called)
            };

            let (output, state_new) = match result.starts_with("(error '") {
                true => (result, state_old),
                false => match result.rfind('.') {
                    Some(index) => match *&result[(index + 16)..(result.len() - 3)]
                        .split(' ')
                        .collect::<Vec<&str>>()
                        .iter()
                        .map(|x| x.parse::<u8>().expect("Failed to parse state byte"))
                        .collect::<Vec<u8>>()
                        .try_into()
                    {
                        Ok(state_new) => (String::from(&result[1..(index - 1)]), state_new),
                        Err(_) => (
                            String::from("(error 'sync-format \"Invalid return format\")"),
                            state_old,
                        ),
                    },
                    None => (
                        String::from("(error 'sync-format \"Invalid return format\")"),
                        state_old,
                    ),
                },
            };

            if external_called && state_old != state_new {
                let output = String::from(
                    "(error 'external-state-error \"Request called an external function and changed state\")",
                );
                warn_on_error_result(output.as_str());
                debug!("Completed request in {:?}; body and result omitted", start.elapsed());
                return output;
            }

            match state_old == state_new {
                true => {
                    warn_on_error_result(output.as_str());
                    debug!("Completed request in {:?}; body and result omitted", start.elapsed());
                    return output;
                }
                false => match state_old
                    == PERSISTOR
                        .root_get(record)
                        .expect("Failed to get record state for comparison")
                {
                    true => {
                        let _lock2 = match _lock1 {
                            Some(_) => None,
                            None => Some(LOCK.lock().expect("Failed to acquire secondary lock")),
                        };

                        match PERSISTOR.root_set(record, state_old, state_new, &persistor) {
                            Ok(_) => {
                                warn_on_error_result(output.as_str());
                                debug!("Completed request in {:?}; body and result omitted", start.elapsed());
                                return output;
                            }
                            Err(_) => {
                                info!("Rerunning request (x{}) due to concurrency collision; body omitted", runs);
                                continue;
                            }
                        }
                    }
                    false => {
                        info!("Rerunning request (x{}) due to concurrency collision; body omitted", runs);
                        continue;
                    }
                },
            }
        }
    }
}

#[cfg(feature = "wasm-kernel")]
static KERNEL_RESPONSE: Lazy<Mutex<Vec<u8>>> = Lazy::new(|| Mutex::new(Vec::new()));

#[cfg(feature = "wasm-kernel")]
fn kernel_evaluate_inner(record: Word, state_old: Word, genesis: &str, query: &str) -> Vec<u8> {
    let evaluator = journal_evaluator();
    unsafe {
        s7::s7_eval_c_string(
            evaluator.sc,
            c"(set! (*s7* 'gc-resize-heap-fraction) 0.1)".as_ptr(),
        );
    }
    install_sync_let_runtime(&evaluator);
    let persistor = MemoryPersistor::new();
    if let Ok((left, right, digest)) = PERSISTOR.branch_get(state_old) {
        persistor
            .branch_set(left, right, digest)
            .expect("copy state root");
        persistor.mark_imported(state_old);
    } else {
        return b"kernel could not resolve state root".to_vec();
    }
    SESSIONS.write().expect("sessions lock").insert(
        evaluator.sc as usize,
        Session::new(
            record,
            state_old,
            persistor.clone(),
            Arc::new(Mutex::new(HashMap::new())),
        ),
    );
    let expression = format!(
        "((eval {}) (sync-state) (read (open-input-string \"{}\")))",
        genesis,
        escape_scheme_string(query),
    );
    let result = evaluator.evaluate(&expression);
    let session = SESSIONS
        .write()
        .expect("sessions lock")
        .remove(&(evaluator.sc as usize))
        .expect("kernel session");
    unsafe {
        if let Some(location) = session.sync_let_base_env_loc {
            s7::s7_gc_unprotect_at(evaluator.sc, location);
        }
        for location in session.serialization_query_locs.values() {
            s7::s7_gc_unprotect_at(evaluator.sc, *location);
        }
    }
    let (output, state_new) = if result.starts_with("(error '") {
        (result, state_old)
    } else if let Some(index) = result.rfind('.') {
        let parsed = result[(index + 16)..(result.len() - 3)]
            .split(' ')
            .map(str::parse::<u8>)
            .collect::<Result<Vec<_>, _>>()
            .ok()
            .and_then(|bytes| Word::try_from(bytes).ok());
        match parsed {
            Some(state) => (String::from(&result[1..(index - 1)]), state),
            None => (
                String::from("(error 'sync-format \"Invalid return format\")"),
                state_old,
            ),
        }
    } else {
        (
            String::from("(error 'sync-format \"Invalid return format\")"),
            state_old,
        )
    };
    let graph = session.persistor.encode_graph(state_new);
    let mut response = Vec::with_capacity(4 + output.len() + SIZE + 1 + 4 + graph.len());
    response.extend(
        u32::try_from(output.len())
            .expect("output bound")
            .to_be_bytes(),
    );
    response.extend(output.as_bytes());
    response.extend(state_new);
    response.push(u8::from(session.external_called));
    response.extend(
        u32::try_from(graph.len())
            .expect("graph bound")
            .to_be_bytes(),
    );
    response.extend(graph);
    response
}

#[cfg(feature = "wasm-kernel")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kernel_alloc(length: usize) -> *mut u8 {
    unsafe { libc::malloc(length.max(1)) as *mut u8 }
}

#[cfg(feature = "wasm-kernel")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kernel_free(pointer: *mut u8) {
    unsafe { libc::free(pointer.cast()) }
}

#[cfg(feature = "wasm-kernel")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kernel_evaluate(pointer: *const u8, length: usize) -> i32 {
    let request = unsafe { std::slice::from_raw_parts(pointer, length) };
    if request.len() < SIZE * 2 + 8 {
        return -1;
    }
    let record: Word = request[..SIZE].try_into().expect("record shape");
    let state: Word = request[SIZE..SIZE * 2].try_into().expect("state shape");
    let genesis_len = u32::from_be_bytes(request[64..68].try_into().unwrap()) as usize;
    let query_len = u32::from_be_bytes(request[68..72].try_into().unwrap()) as usize;
    if 72usize
        .checked_add(genesis_len)
        .and_then(|n| n.checked_add(query_len))
        != Some(request.len())
    {
        return -3;
    }
    let Ok(genesis) = std::str::from_utf8(&request[72..72 + genesis_len]) else {
        return -4;
    };
    let Ok(query) = std::str::from_utf8(&request[72 + genesis_len..]) else {
        return -5;
    };
    *KERNEL_RESPONSE.lock().expect("response lock") =
        kernel_evaluate_inner(record, state, genesis, query);
    0
}

#[cfg(feature = "wasm-kernel")]
#[unsafe(no_mangle)]
pub extern "C" fn kernel_result_pointer() -> *const u8 {
    KERNEL_RESPONSE.lock().expect("response lock").as_ptr()
}

#[cfg(feature = "wasm-kernel")]
#[unsafe(no_mangle)]
pub extern "C" fn kernel_result_length() -> usize {
    KERNEL_RESPONSE.lock().expect("response lock").len()
}

pub(crate) unsafe fn sync_error(sc: *mut s7::s7_scheme, string: &str) -> s7::s7_pointer {
    unsafe {
        let c_string = CString::new(string).expect("Failed to create CString from string");

        s7::s7_error(
            sc,
            s7::s7_make_symbol(sc, c"sync-web-error".as_ptr()),
            s7::s7_list(sc, 1, s7::s7_make_string(sc, c_string.as_ptr())),
        )
    }
}

fn mark_external_called(sc: *mut s7::s7_scheme) {
    let mut sessions = SESSIONS
        .write()
        .expect("Failed to acquire sessions lock for external call tracking");
    let session = sessions
        .get_mut(&(sc as usize))
        .expect("Session not found for given context");
    session.external_called = true;
}

#[cfg(test)]
mod transport_tests {
    use super::{http_client, remote_post};
    use std::time::Duration;

    #[test]
    fn remote_transport_fails_closed_and_recovers() {
        let mut server = mockito::Server::new();
        let ok = server
            .mock("POST", "/ok")
            .with_status(200)
            .with_body("ok")
            .expect(2)
            .create();
        let redirected_ok = server
            .mock("GET", "/ok")
            .with_status(200)
            .with_body("ok")
            .expect(0)
            .create();
        let empty = server.mock("POST", "/empty").with_status(204).create();
        let failure = server
            .mock("POST", "/failure")
            .with_status(503)
            .with_body("ignored")
            .create();
        let redirect = server
            .mock("POST", "/redirect")
            .with_status(302)
            .with_header("location", "/ok")
            .create();
        let slow = server
            .mock("POST", "/slow")
            .with_status(200)
            .with_chunked_body(|writer| {
                std::thread::sleep(Duration::from_millis(600));
                writer.write_all(b"late")
            })
            .create();
        let client = http_client(Duration::from_millis(250));
        let runtime = tokio::runtime::Runtime::new().expect("runtime");

        assert_eq!(
            runtime
                .block_on(remote_post(
                    &client,
                    &format!("{}/ok", server.url()),
                    "request".into()
                ))
                .expect("successful response"),
            b"ok"
        );
        assert_eq!(
            runtime
                .block_on(remote_post(
                    &client,
                    &format!("{}/empty", server.url()),
                    "request".into()
                ))
                .expect("empty 2xx response"),
            b""
        );
        assert!(
            runtime
                .block_on(remote_post(
                    &client,
                    &format!("{}/failure", server.url()),
                    "request".into()
                ))
                .is_err()
        );
        assert!(
            runtime
                .block_on(remote_post(
                    &client,
                    &format!("{}/redirect", server.url()),
                    "request".into()
                ))
                .is_err()
        );
        assert!(
            runtime
                .block_on(remote_post(
                    &client,
                    &format!("{}/slow", server.url()),
                    "request".into()
                ))
                .is_err()
        );
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(
            runtime
                .block_on(remote_post(
                    &client,
                    &format!("{}/ok", server.url()),
                    "retry".into()
                ))
                .expect("successful retry"),
            b"ok"
        );

        ok.assert();
        redirected_ok.assert();
        empty.assert();
        failure.assert();
        redirect.assert();
        slow.assert();
    }
}
