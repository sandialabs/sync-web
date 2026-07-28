#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

use crate::cache::{
    ResolveSource, ResolvedNode, resolve_branch_with, resolve_node_with, resolve_stump_with,
};
pub use crate::config::Config;
use crate::evaluator::{Evaluator, Primitive, Type, json2lisp, lisp2json, obj2str};
use crate::extensions::crypto::{
    primitive_s7_crypto_generate, primitive_s7_crypto_sign, primitive_s7_crypto_verify,
};
use crate::extensions::system::{primitive_s7_system_time_unix, primitive_s7_system_time_utc};
use crate::persistor::{MemoryPersistor, PERSISTOR, Persistor};
pub use crate::persistor::{SIZE, Word};
use crate::scenario_context::ScenarioContext;
use crate::serialization::{
    primitive_s7_sync_deserialize, primitive_s7_sync_serialize, serialization_trace_active,
    serialization_trace_child, serialization_trace_pair,
};
use libc;
use log::{debug, info, warn};
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use evaluator as s7;
use sha2::{Digest, Sha256};
use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_char;

mod cache;
mod config;
pub mod evaluator;
mod persistor;
mod scenario_context;
mod serialization;
#[cfg(feature = "test-support")]
pub mod test_support;
mod extensions {
    pub mod crypto;
    pub mod system;
}

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
    pub(crate) serialization_env_loc: Option<s7::s7_int>,
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
            serialization_env_loc: None,
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

static LOCK: Mutex<()> = Mutex::new(());
static RUNS: usize = 1;

fn escape_scheme_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\"', "\\\"")
}

fn truncate_for_log(value: &str, limit: usize) -> String {
    let truncated: String = value.chars().take(limit).collect();
    if value.chars().count() > limit {
        format!("{truncated} ...")
    } else {
        truncated
    }
}

fn warn_on_error_result(query: &str, output: &str) {
    if output.starts_with("(error ") {
        warn!(
            "Evaluation returned error form. Query: {} Result: {}",
            truncate_for_log(query, 256),
            truncate_for_log(output, 256),
        );
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
pub struct Journal {
    client: reqwest::Client,
}

impl Journal {
    fn new() -> Self {
        match PERSISTOR.root_new(
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
        ) {
            Ok(_) => Self {
                client: reqwest::Client::new(),
            },
            Err(_) => Self {
                client: reqwest::Client::new(),
            },
        }
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
                        log::warn!("Failed to parse Scheme to JSON. Result: {}", result);
                        lisp2json("(error 'parse-error \"Failed to parse Scheme to JSON\")")
                    }
                    .expect("Error parsing the JSON error message"),
                }
            }
            Err(_) => {
                let query_str = serde_json::to_string(&query)
                    .unwrap_or_else(|_| "<unprintable json>".to_string());
                log::warn!("Failed to parse JSON to Scheme. Query: {}", query_str);
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
                log::warn!("Failed to parse Scheme to JSON. Query: {}", query);
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
                let query_str = serde_json::to_string(&query)
                    .unwrap_or_else(|_| "<unprintable json>".to_string());
                log::warn!("Failed to parse JSON to Scheme. Query: {}", query_str);
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
        let mut runs = 0;
        let cache = Arc::new(Mutex::new(HashMap::new()));

        let start = Instant::now();
        debug!(
            "Evaluating ({})",
            query.chars().take(128).collect::<String>(),
        );

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

            let evaluator = Evaluator::new(
                vec![(SYNC_NODE_TAG, type_s7_sync_node())]
                    .into_iter()
                    .collect(),
                vec![
                    primitive_s7_sync_hash(),
                    primitive_s7_sync_null(),
                    primitive_s7_sync_state(),
                    primitive_s7_sync_stub(),
                    primitive_s7_sync_is_node(),
                    primitive_s7_sync_is_pair(),
                    primitive_s7_sync_is_stub(),
                    primitive_s7_sync_is_null(),
                    primitive_s7_sync_digest(),
                    primitive_s7_sync_cons(),
                    primitive_s7_sync_car(),
                    primitive_s7_sync_cdr(),
                    primitive_s7_sync_cut(),
                    primitive_s7_sync_create(),
                    primitive_s7_sync_delete(),
                    primitive_s7_sync_all(),
                    primitive_s7_sync_call(),
                    primitive_s7_sync_eval(),
                    primitive_s7_sync_serialize(),
                    primitive_s7_sync_deserialize(),
                    primitive_s7_sync_safe_setter(),
                    primitive_s7_sync_safe_setter_set(),
                    primitive_s7_sync_let_active(),
                    primitive_s7_sync_let_eval(),
                    primitive_s7_sync_let(),
                    primitive_s7_sync_let_return(),
                    primitive_s7_sync_remote(),
                    primitive_s7_sync_http(),
                    primitive_s7_crypto_generate(),
                    primitive_s7_crypto_sign(),
                    primitive_s7_crypto_verify(),
                    primitive_s7_system_time_unix(),
                    primitive_s7_system_time_utc(),
                ],
            );

            let sync_let = CString::new(
                "(define-macro (sync-let bindings . body)\
                   (if (or (not (list? bindings)) (null? body))\
                       (error 'syntax-error \"sync-let requires a binding list and body\"))\
                   (for-each\
                    (lambda (binding)\
                      (if (not (and (list? binding) (= (length binding) 2)\
                                    (symbol? (car binding))))\
                          (error 'syntax-error \"Malformed sync-let binding: ~S\" binding)))\
                    bindings)\
                   `(%sync-let-return\
                     (%sync-let ',(map car bindings)\
                                (list ,@(map cadr bindings))\
                                (expression->byte-vector '(begin ,@body))))))",
            )
            .expect("Failed to construct sync-let macro");
            unsafe {
                let safe_setter = s7::s7_let_ref(
                    evaluator.sc,
                    s7::s7_rootlet(evaluator.sc),
                    s7::s7_make_symbol(evaluator.sc, c"%sync-safe-setter".as_ptr()),
                );
                let safe_setter_set = s7::s7_let_ref(
                    evaluator.sc,
                    s7::s7_rootlet(evaluator.sc),
                    s7::s7_make_symbol(evaluator.sc, c"%sync-safe-setter-set!".as_ptr()),
                );
                s7::s7_set_setter(evaluator.sc, safe_setter, safe_setter_set);
                s7::s7_eval_c_string(evaluator.sc, sync_let.as_ptr());
                for name in [
                    c"%sync-safe-setter-set!",
                    c"%sync-let-body",
                    c"%sync-let-environment",
                    c"%sync-let-eval",
                    c"%sync-let-ok",
                    c"%sync-let-error",
                    c"%sync-let-list",
                ] {
                    s7::s7_define(
                        evaluator.sc,
                        s7::s7_rootlet(evaluator.sc),
                        s7::s7_make_symbol(evaluator.sc, name.as_ptr()),
                        s7::s7_undefined(evaluator.sc),
                    );
                }
            }

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
                        if let Some(location) = session.serialization_env_loc {
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
                warn_on_error_result(query, output.as_str());
                debug!(
                    "Completed ({:?}) {} -> {}",
                    start.elapsed(),
                    query.chars().take(128).collect::<String>(),
                    output,
                );
                return output;
            }

            match state_old == state_new {
                true => {
                    warn_on_error_result(query, output.as_str());
                    debug!(
                        "Completed ({:?}) {} -> {}",
                        start.elapsed(),
                        query.chars().take(128).collect::<String>(),
                        output,
                    );
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
                                    warn_on_error_result(query, output.as_str());
                                    debug!(
                                        "Completed ({:?}) {} -> {}",
                                        start.elapsed(),
                                        query.chars().take(128).collect::<String>(),
                                        output,
                                    );
                                    return output;
                                }
                                Err(_) => {
                                    info!(
                                        "Rerunning (x{}) due to concurrency collision: {}",
                                        runs,
                                        query.chars().take(128).collect::<String>(),
                                    );
                                    continue;
                                }
                            }
                        }
                    false => {
                        info!(
                            "Rerunning (x{}) due to concurrency collision: {}",
                            runs,
                            query.chars().take(128).collect::<String>(),
                        );
                        continue;
                    }
                },
            }
        }
    }
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

fn type_s7_sync_node() -> Type {
    unsafe extern "C" fn free(_sc: *mut s7::s7_scheme, obj: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            sync_heap_free(s7::s7_c_object_value(obj));
            std::ptr::null_mut()
        }
    }

    unsafe extern "C" fn mark(_sc: *mut s7::s7_scheme, _obj: s7::s7_pointer) -> s7::s7_pointer {
        std::ptr::null_mut()
    }

    unsafe extern "C" fn is_equal(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            match sync_is_node(s7::s7_cadr(args)) {
                true => {
                    let word1 = sync_heap_read(s7::s7_c_object_value(s7::s7_car(args)));
                    let word2 = sync_heap_read(s7::s7_c_object_value(s7::s7_cadr(args)));
                    s7::s7_make_boolean(sc, word1 == word2)
                }
                false => s7::s7_wrong_type_arg_error(
                    sc,
                    c"equal?".as_ptr(),
                    2,
                    s7::s7_cadr(args),
                    c"a sync-node".as_ptr(),
                ),
            }
        }
    }

    unsafe extern "C" fn to_string(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            string_to_s7(
                sc,
                format!(
                    "(sync-node #u({}))",
                    sync_heap_read(s7::s7_c_object_value(s7::s7_car(args)))
                        .iter()
                        .map(|&byte| byte.to_string())
                        .collect::<Vec<String>>()
                        .join(" "),
                )
                .as_str(),
            )
        }
    }

    Type::new(c"sync-node", free, mark, is_equal, to_string)
}

fn primitive_s7_sync_stub() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let bv = s7::s7_car(args);

            if !s7::s7_is_byte_vector(bv) || s7::s7_vector_length(bv) as usize != SIZE {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-cut".as_ptr(),
                    1,
                    s7::s7_car(args),
                    c"a hash-sized byte-vector".as_ptr(),
                );
            }

            let mut digest = [0 as u8; SIZE];
            for i in 0..SIZE {
                digest[i] = s7::s7_byte_vector_ref(bv, i as i64);
            }

            let persistor = {
                let session = SESSIONS.read().expect("Failed to acquire SESSIONS lock");
                &session
                    .get(&(sc as usize))
                    .expect("Session not found for given context")
                    .persistor
                    .clone()
            };

            match persistor.stump_set(digest) {
                Ok(stump) => s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(stump)),
                Err(_) => sync_error(sc, "Journal is unable to create stub node (sync-stub)"),
            }
        }
    }

    Primitive::new(
        code,
        c"sync-stub",
        c"(sync-stub digest) create a sync stub from the provided byte-vector",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_hash() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let data_bv = s7::s7_car(args);

            // check the input arguments
            if !s7::s7_is_byte_vector(data_bv) {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-hash".as_ptr(),
                    1,
                    data_bv,
                    c"a byte-vector".as_ptr(),
                );
            }

            // convert to rust data types
            let mut data = vec![];
            for i in 0..s7::s7_vector_length(data_bv) {
                data.push(s7::s7_byte_vector_ref(data_bv, i as i64))
            }

            let digest = Sha256::digest(data).to_vec();
            let digest_bv = s7::s7_make_byte_vector(sc, SIZE as i64, 1, std::ptr::null_mut());
            for i in 0..SIZE {
                s7::s7_byte_vector_set(digest_bv, i as i64, digest[i]);
            }
            digest_bv
        }
    }

    Primitive::new(
        code,
        c"sync-hash",
        c"(sync-hash bv) compute the SHA-256 digest of a byte vector",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_state() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            if !s7::s7_is_null(sc, args) {
                return s7::s7_wrong_number_of_args_error(sc, c"sync-state".as_ptr(), args);
            }

            let state = {
                let session = SESSIONS.read().expect("Failed to acquire sessions lock");
                session
                    .get(&(sc as usize))
                    .expect("Session not found for sync-state")
                    .state
            };

            s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(state))
        }
    }

    Primitive::new(
        code,
        c"sync-state",
        c"(sync-state) returns the current session state as a sync-node",
        0,
        0,
        false,
    )
}

fn primitive_s7_sync_is_node() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe { s7::s7_make_boolean(sc, sync_is_node(s7::s7_car(args))) }
    }

    Primitive::new(
        code,
        c"sync-node?",
        c"(sync-node?) returns whether the object is a sync node",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_null() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, _args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe { s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(NULL)) }
    }

    Primitive::new(
        code,
        c"sync-null",
        c"(sync-null) returns the null synchronic node",
        0,
        0,
        false,
    )
}

fn primitive_s7_sync_is_null() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let arg = s7::s7_car(args);
            if sync_is_node(arg) {
                let word = sync_heap_read(s7::s7_c_object_value(arg));
                for i in 0..SIZE {
                    if word[i] != 0 {
                        return s7::s7_make_boolean(sc, false);
                    }
                }
                s7::s7_make_boolean(sc, true)
            } else if s7::s7_is_byte_vector(arg) {
                s7::s7_make_boolean(sc, false)
            } else {
                s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-null?".as_ptr(),
                    1,
                    arg,
                    c"a sync-node or byte-vector".as_ptr(),
                )
            }
        }
    }

    Primitive::new(
        code,
        c"sync-null?",
        c"(sync-null? sp) returns whether a sync-node or byte-vector is equal to sync-null",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_is_pair() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let arg = s7::s7_car(args);
            if sync_is_node(arg) {
                let word = sync_heap_read(s7::s7_c_object_value(arg));
                s7::s7_make_boolean(sc, sync_branch_children(sc, word).is_ok())
            } else if s7::s7_is_byte_vector(arg) {
                s7::s7_make_boolean(sc, false)
            } else {
                s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-pair?".as_ptr(),
                    1,
                    arg,
                    c"a sync-node or byte-vector".as_ptr(),
                )
            }
        }
    }

    Primitive::new(
        code,
        c"sync-pair?",
        c"(sync-pair? sp) returns whether a sync-node or byte-vector is a pair",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_is_stub() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let arg = s7::s7_car(args);
            if sync_is_node(arg) {
                let word = sync_heap_read(s7::s7_c_object_value(arg));
                let persistor = session_persistor_for(sc);
                s7::s7_make_boolean(sc, resolve_stump_with(&persistor, word).is_some())
            } else if s7::s7_is_byte_vector(arg) {
                s7::s7_make_boolean(sc, false)
            } else {
                s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-stub?".as_ptr(),
                    1,
                    arg,
                    c"a sync-node or byte-vector".as_ptr(),
                )
            }
        }
    }

    Primitive::new(
        code,
        c"sync-stub?",
        c"(sync-stub? sp) returns whether a sync-node or byte-vector is a stub",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_digest() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let arg = s7::s7_car(args);
            if sync_is_node(arg) {
                let word = sync_heap_read(s7::s7_c_object_value(arg));
                let digest = sync_digest(sc, word).expect("Failed to obtain digest");
                let bv = s7::s7_make_byte_vector(sc, SIZE as i64, 1, std::ptr::null_mut());
                for i in 0..SIZE {
                    s7::s7_byte_vector_set(bv, i as i64, digest[i]);
                }
                bv
            } else if s7::s7_is_byte_vector(arg) {
                let mut data = vec![];
                for i in 0..s7::s7_vector_length(arg) {
                    data.push(s7::s7_byte_vector_ref(arg, i as i64))
                }
                let digest = Sha256::digest(data);
                let bv = s7::s7_make_byte_vector(sc, SIZE as i64, 1, std::ptr::null_mut());
                for i in 0..SIZE {
                    s7::s7_byte_vector_set(bv, i as i64, digest[i]);
                }
                bv
            } else {
                s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-digest".as_ptr(),
                    1,
                    arg,
                    c"a sync-node or byte-vector".as_ptr(),
                )
            }
        }
    }

    Primitive::new(
        code,
        c"sync-digest",
        c"(sync-digest value) returns the digest of a sync-node or byte-vector as a byte-vector",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_cons() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let persistor = {
                let session = SESSIONS.read().expect("Failed to acquire sessions lock");
                &session
                    .get(&(sc as usize))
                    .expect("Session not found for sync-cons")
                    .persistor
                    .clone()
            };

            let handle_arg = |obj, number| {
                if sync_is_node(obj) {
                    Ok(sync_heap_read(s7::s7_c_object_value(obj)))
                } else if s7::s7_is_byte_vector(obj) {
                    let mut content = vec![];
                    for i in 0..s7::s7_vector_length(obj) {
                        content.push(s7::s7_byte_vector_ref(obj, i as i64))
                    }
                    match persistor.leaf_set(content) {
                        Ok(atom) => Ok(atom),
                        Err(_) => Err(sync_error(
                            sc,
                            "Journal is unable to add leaf node (sync-cons)",
                        )),
                    }
                } else {
                    Err(s7::s7_wrong_type_arg_error(
                        sc,
                        c"sync-cons".as_ptr(),
                        number,
                        obj,
                        c"a byte vector or a sync node".as_ptr(),
                    ))
                }
            };

            match (
                handle_arg(s7::s7_car(args), 1),
                handle_arg(s7::s7_cadr(args), 2),
            ) {
                (Ok(left), Ok(right)) => match (sync_digest(sc, left), sync_digest(sc, right)) {
                    (Ok(digest_left), Ok(digest_right)) => {
                        let mut joined = [0 as u8; SIZE * 2];
                        joined[..SIZE].copy_from_slice(&digest_left);
                        joined[SIZE..].copy_from_slice(&digest_right);
                        let digest = Word::from(Sha256::digest(joined));

                        match persistor.branch_set(left, right, digest) {
                            Ok(pair) => {
                                serialization_trace_pair(sc, pair, left, right);
                                s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(pair))
                            }
                            Err(_) => {
                                sync_error(sc, "Journal is unable to add pair node (sync-cons)")
                            }
                        }
                    }
                    _ => sync_error(sc, "Journal is unable to obtain node digests (sync-cons)"),
                },
                (Err(left), _) => left,
                (_, Err(right)) => right,
            }
        }
    }

    Primitive::new(
        code,
        c"sync-cons",
        c"(sync-cons first rest) construct a new sync pair node",
        2,
        0,
        false,
    )
}

fn primitive_s7_sync_car() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            if !sync_is_node(s7::s7_car(args)) {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-car".as_ptr(),
                    1,
                    s7::s7_car(args),
                    c"a sync-pair".as_ptr(),
                );
            }
            sync_cxr(sc, args, c"sync-car", true, |children| children.0)
        }
    }

    Primitive::new(
        code,
        c"sync-car",
        c"(sync-car pair) retrieve the first element of a sync pair",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_cdr() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            if !sync_is_node(s7::s7_car(args)) {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-cdr".as_ptr(),
                    1,
                    s7::s7_car(args),
                    c"a sync-pair".as_ptr(),
                );
            }
            sync_cxr(sc, args, c"sync-cdr", false, |children| children.1)
        }
    }

    Primitive::new(
        code,
        c"sync-cdr",
        c"(sync-cdr pair) retrieve the second element of a sync pair",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_cut() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let arg = s7::s7_car(args);

            let handle_digest = |digest| {
                let persistor = {
                    let session = SESSIONS.read().expect("Failed to acquire SESSIONS lock");
                    &session
                        .get(&(sc as usize))
                        .expect("Session not found for given context")
                        .persistor
                        .clone()
                };
                match persistor.stump_set(digest) {
                    Ok(stump) => s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(stump)),
                    Err(_) => sync_error(sc, "Journal is unable to add stub node (sync-cut)"),
                }
            };

            if s7::s7_is_byte_vector(arg) {
                let mut content = vec![];
                for i in 0..s7::s7_vector_length(arg) {
                    content.push(s7::s7_byte_vector_ref(arg, i as i64))
                }
                handle_digest(Word::from(Sha256::digest(Sha256::digest(&content))))
            } else if sync_is_node(arg) {
                match sync_digest(sc, sync_heap_read(s7::s7_c_object_value(arg))) {
                    Ok(digest) => handle_digest(digest),
                    Err(_) => sync_error(sc, "Journal does not recognize input node (sync-cut)"),
                }
            } else {
                s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-cut".as_ptr(),
                    1,
                    s7::s7_car(args),
                    c"a sync-node or byte-vector".as_ptr(),
                )
            }
        }
    }

    Primitive::new(
        code,
        c"sync-cut",
        c"(sync-cut value) obtain the stub of a sync-node",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_create() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let id = s7::s7_car(args);

            if !s7::s7_is_byte_vector(id) || s7::s7_vector_length(id) as usize != SIZE {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-create".as_ptr(),
                    1,
                    id,
                    c"a hash-sized byte-vector".as_ptr(),
                );
            }

            let mut record: Word = [0 as u8; SIZE];

            for i in 0..SIZE {
                record[i as usize] = s7::s7_byte_vector_ref(id, i as i64)
            }

            debug!("Adding record: {}", hex::encode(record));

            match PERSISTOR.root_new(
                record,
                PERSISTOR
                    .branch_set(
                        PERSISTOR
                            .leaf_set(GENESIS_STR.as_bytes().to_vec())
                            .expect("Failed to create genesis leaf for new record"),
                        NULL,
                        NULL,
                    )
                    .expect("Failed to create genesis branch for new record"),
            ) {
                Ok(_) => s7::s7_make_boolean(sc, true),
                Err(_) => s7::s7_error(
                    sc,
                    s7::s7_make_symbol(sc, c"sync-web-error".as_ptr()),
                    s7::s7_list(
                        sc,
                        1,
                        s7::s7_make_string(sc, c"record ID is already in use".as_ptr()),
                    ),
                ),
            }
        }
    }

    Primitive::new(
        code,
        c"sync-create",
        c"(sync-create id) create a new synchronic record with the given 32-byte ID",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_delete() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let id = s7::s7_car(args);

            if !s7::s7_is_byte_vector(id) || s7::s7_vector_length(id) as usize != SIZE {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-delete".as_ptr(),
                    1,
                    id,
                    c"a hash-sized byte-vector".as_ptr(),
                );
            }

            let mut record: Word = [0 as u8; SIZE];

            for i in 0..s7::s7_vector_length(id) {
                record[i as usize] = s7::s7_byte_vector_ref(id, i as i64)
            }

            if record == NULL {
                return s7::s7_error(
                    sc,
                    s7::s7_make_symbol(sc, c"sync-web-error".as_ptr()),
                    s7::s7_list(
                        sc,
                        1,
                        s7::s7_make_string(sc, c"cannot delete the root record".as_ptr()),
                    ),
                );
            }

            debug!("Deleting record: {}", hex::encode(record));

            match PERSISTOR.root_delete(record) {
                Ok(_) => s7::s7_make_boolean(sc, true),
                Err(_) => s7::s7_error(
                    sc,
                    s7::s7_make_symbol(sc, c"sync-web-error".as_ptr()),
                    s7::s7_list(
                        sc,
                        1,
                        s7::s7_make_string(sc, c"record ID does not exist".as_ptr()),
                    ),
                ),
            }
        }
    }

    Primitive::new(
        code,
        c"sync-delete",
        c"(sync-delete id) delete the synchronic record with the given 32-byte ID",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_all() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, _args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let mut list = s7::s7_list(sc, 0);

            for record in PERSISTOR.root_list().into_iter().rev() {
                let bv = s7::s7_make_byte_vector(sc, SIZE as i64, 1, std::ptr::null_mut());
                for i in 0..SIZE {
                    s7::s7_byte_vector_set(bv, i as i64, record[i]);
                }

                list = s7::s7_cons(sc, bv, list)
            }

            list
        }
    }

    Primitive::new(
        code,
        c"sync-all",
        c"(sync-all) list all synchronic record IDs in ascending order",
        0,
        0,
        false,
    )
}

fn primitive_s7_sync_call() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            mark_external_called(sc);

            let message_expr = s7::s7_car(args);
            let blocking = s7::s7_cadr(args);

            if !s7::s7_is_boolean(blocking) {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-call".as_ptr(),
                    2,
                    blocking,
                    c"a boolean".as_ptr(),
                );
            }

            let record = match s7::s7_is_null(sc, s7::s7_cddr(args)) {
                true => {
                    let session = SESSIONS.read().expect("Failed to acquire sessions lock");
                    session
                        .get(&(sc as usize))
                        .expect("Session number not found in sessions map")
                        .record
                }
                false => {
                    let bv = s7::s7_caddr(args);
                    // check the input arguments
                    if !s7::s7_is_byte_vector(bv) || s7::s7_vector_length(bv) as usize != SIZE {
                        return s7::s7_wrong_type_arg_error(
                            sc,
                            c"sync-call".as_ptr(),
                            3,
                            bv,
                            c"a hash-sized byte-vector".as_ptr(),
                        );
                    }

                    let mut record = [0 as u8; SIZE];
                    for i in 0..SIZE {
                        record[i] = s7::s7_byte_vector_ref(bv, i as i64);
                    }
                    record
                }
            };

            match PERSISTOR.root_get(record) {
                Ok(_) => {
                    let message = obj2str(sc, message_expr);
                    let scenario = SESSIONS
                        .read()
                        .expect("Failed to acquire sessions lock")
                        .get(&(sc as usize))
                        .and_then(|session| session.scenario.clone());
                    if s7::s7_boolean(sc, blocking) {
                        let result = JOURNAL.evaluate_record_with_context(
                            record,
                            message.as_str(),
                            scenario,
                        );
                        let c_result = CString::new(format!("(quote {})", result))
                            .expect("Failed to create C string from journal evaluation result");
                        s7::s7_eval_c_string(sc, c_result.as_ptr())
                    } else {
                        if scenario.is_some() {
                            JOURNAL.evaluate_record_with_context(
                                record,
                                message.as_str(),
                                scenario,
                            );
                    } else {
                        tokio::spawn(async move {
                            JOURNAL.evaluate_record(record, message.as_str());
                        });
                        }
                        s7::s7_make_boolean(sc, true)
                    }
                }
                Err(_) => s7::s7_error(
                    sc,
                    s7::s7_make_symbol(sc, c"sync-web-error".as_ptr()),
                    s7::s7_list(
                        sc,
                        1,
                        s7::s7_make_string(sc, c"record ID does not exist".as_ptr()),
                    ),
                ),
            }
        }
    }

    Primitive::new(
        code,
        c"sync-call",
        c"(sync-call query blocking? id) query the provided record ID or self if ID not provided",
        2,
        1,
        false,
    )
}

const SYNC_LET_ALLOWED: &[&str] = &[
    "*", "+", "-", "/", "<", "<=", "=", ">", ">=", "and", "append", "apply",
    "apply-values", "ash", "assq", "assoc", "begin", "boolean?", "byte-vector?",
    "byte-vector->expression", "byte-vector->hex-string", "byte-vector-length",
    "byte-vector-ref", "byte-vector-set!", "caadar", "caadr", "caar", "cadar", "cadr", "caddr",
    "car", "case", "catch", "cdadar", "cddar", "cddr", "cdr", "char?", "complex?",
    "cond", "cons", "define", "define*", "do", "else", "eq?", "equal?", "eqv?",
    "error", "even?", "expt", "expression->byte-vector", "for-each", "if", "integer?",
    "keyword?", "lambda", "lambda*", "length", "let", "let*", "letrec", "letrec*",
    "list", "list-tail", "list-values", "list?", "logand", "macro?", "make-list", "map",
    "max", "member", "memq", "min", "modulo", "negative?", "not", "null?",
    "number->string", "number?", "odd?", "or", "pair?", "positive?", "procedure?",
    "proper-list?", "quasiquote", "quote", "rational?", "real?", "remainder", "reverse",
    "set!", "string->symbol", "string?", "string=?", "string-length", "string-ref",
    "substring", "subvector", "symbol->string", "symbol?", "sync-car", "sync-cdr",
    "sync-cons", "sync-cut", "sync-deserialize", "sync-digest", "sync-eval", "sync-hash",
    "sync-let-active?", "sync-let-eval", "sync-node?", "sync-null", "sync-null?",
    "sync-pair?", "sync-serialize", "sync-stub", "sync-stub?", "throw", "unquote",
    "unquote-splicing", "vector", "vector-length", "vector-ref", "vector-set!", "vector?",
    "zero?",
];

const SERIALIZATION_QUERY_ADDITIONAL: &[&str] = &[
    "ash", "cadr", "caddr", "expt", "list-values", "logand", "make-list",
    "subvector",
];
enum SyncLetCopyTask {
    Visit(s7::s7_pointer),
    FinishVector { address: usize, length: s7::s7_int },
    FinishList { addresses: Vec<usize>, length: usize },
}

struct SyncLetProtected {
    value: s7::s7_pointer,
    location: s7::s7_int,
}

unsafe fn sync_let_protect(
    sc: *mut s7::s7_scheme,
    value: s7::s7_pointer,
) -> SyncLetProtected {
    unsafe {
        SyncLetProtected {
            value,
            location: s7::s7_gc_protect(sc, value),
        }
    }
}

unsafe fn sync_let_unprotect(sc: *mut s7::s7_scheme, protected: SyncLetProtected) {
    unsafe {
        s7::s7_gc_unprotect_at(sc, protected.location);
    }
}

unsafe fn sync_let_copy_cleanup(
    sc: *mut s7::s7_scheme,
    results: &mut Vec<SyncLetProtected>,
) {
    unsafe {
        for result in results.drain(..) {
            sync_let_unprotect(sc, result);
        }
    }
}

unsafe fn sync_let_copy(
    sc: *mut s7::s7_scheme,
    value: s7::s7_pointer,
    visiting: &mut HashSet<usize>,
    allowed_syntax: Option<&HashSet<usize>>,
) -> Result<SyncLetProtected, String> {
    unsafe {
        let mut tasks = vec![SyncLetCopyTask::Visit(value)];
        let mut results = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                SyncLetCopyTask::Visit(value) => {
                    if value == s7::s7_undefined(sc)
                        || value == s7::s7_unspecified(sc)
                        || value == s7::s7_eof_object(sc)
                    {
                        sync_let_copy_cleanup(sc, &mut results);
                        return Err(
                            "sync-let does not accept undefined, unspecified, or eof values"
                                .to_string(),
                        );
                    }
                    if s7::s7_is_syntax(value) {
                        if allowed_syntax
                            .is_some_and(|allowed| allowed.contains(&(value as usize)))
                        {
                            results.push(sync_let_protect(sc, value));
                            continue;
                        }
                        sync_let_copy_cleanup(sc, &mut results);
                        return Err("sync-let body contains unavailable syntax".to_string());
                    }
                    if sync_is_node(value)
                        || s7::s7_is_null(sc, value)
                        || s7::s7_is_boolean(value)
                        || s7::s7_is_number(value)
                        || s7::s7_is_character(value)
                        || s7::s7_is_symbol(value)
                        || s7::s7_is_keyword(value)
                    {
                        results.push(sync_let_protect(sc, value));
                        continue;
                    }
                    if s7::s7_is_string(value) {
                        results.push(sync_let_protect(
                            sc,
                            s7::s7_make_string_with_length(
                                sc,
                                s7::s7_string(value),
                                s7::s7_string_length(value),
                            ),
                        ));
                        continue;
                    }
                    if s7::s7_is_byte_vector(value) {
                        let length = s7::s7_vector_length(value);
                        let copy = sync_let_protect(
                            sc,
                            s7::s7_make_byte_vector(sc, length, 1, std::ptr::null_mut()),
                        );
                        for index in 0..length {
                            s7::s7_byte_vector_set(
                                copy.value,
                                index,
                                s7::s7_byte_vector_ref(value, index),
                            );
                        }
                        results.push(copy);
                        continue;
                    }
                    if s7::s7_is_vector(value) {
                        let address = value as usize;
                        if !visiting.insert(address) {
                            sync_let_copy_cleanup(sc, &mut results);
                            return Err("sync-let does not accept cyclic vectors".to_string());
                        }
                        let length = s7::s7_vector_length(value);
                        tasks.push(SyncLetCopyTask::FinishVector { address, length });
                        for index in (0..length).rev() {
                            tasks.push(SyncLetCopyTask::Visit(s7::s7_vector_ref(
                                sc, value, index,
                            )));
                        }
                        continue;
                    }
                    if s7::s7_is_pair(value) {
                        let mut source = value;
                        let mut addresses = Vec::new();
                        let mut items = Vec::new();
                        while s7::s7_is_pair(source) {
                            let address = source as usize;
                            if !visiting.insert(address) {
                                sync_let_copy_cleanup(sc, &mut results);
                                return Err("sync-let does not accept cyclic lists".to_string());
                            }
                            addresses.push(address);
                            items.push(s7::s7_car(source));
                            source = s7::s7_cdr(source);
                        }
                        if !s7::s7_is_null(sc, source) {
                            sync_let_copy_cleanup(sc, &mut results);
                            return Err("sync-let accepts only proper lists".to_string());
                        }
                        let length = items.len();
                        tasks.push(SyncLetCopyTask::FinishList { addresses, length });
                        for item in items.into_iter().rev() {
                            tasks.push(SyncLetCopyTask::Visit(item));
                        }
                        continue;
                    }
                    sync_let_copy_cleanup(sc, &mut results);
                    return Err("sync-let values must be inert data or sync nodes".to_string());
                }
                SyncLetCopyTask::FinishVector { address, length } => {
                    let copy = sync_let_protect(sc, s7::s7_make_vector(sc, length));
                    for index in (0..length).rev() {
                        let item = results.pop().expect("sync-let vector copy result missing");
                        s7::s7_vector_set(sc, copy.value, index, item.value);
                        sync_let_unprotect(sc, item);
                    }
                    visiting.remove(&address);
                    results.push(copy);
                }
                SyncLetCopyTask::FinishList { addresses, length } => {
                    let mut copy: Option<SyncLetProtected> = None;
                    for _ in 0..length {
                        let item = results.pop().expect("sync-let list copy result missing");
                        let cell = sync_let_protect(
                            sc,
                            s7::s7_cons(
                                sc,
                                item.value,
                                copy.as_ref().map_or(s7::s7_nil(sc), |value| value.value),
                            ),
                        );
                        sync_let_unprotect(sc, item);
                        if let Some(previous) = copy {
                            sync_let_unprotect(sc, previous);
                        }
                        copy = Some(cell);
                    }
                    for address in addresses {
                        visiting.remove(&address);
                    }
                    results.push(copy.unwrap_or_else(|| sync_let_protect(sc, s7::s7_nil(sc))));
                }
            }
        }
        if results.len() != 1 {
            sync_let_copy_cleanup(sc, &mut results);
            return Err("sync-let boundary copy produced an invalid result".to_string());
        }
        Ok(results.pop().expect("sync-let copy result missing"))
    }
}

unsafe fn sync_let_failure(
    sc: *mut s7::s7_scheme,
    message: &str,
) -> s7::s7_pointer {
    unsafe {
        let message = CString::new(message).unwrap_or_else(|_| {
            CString::new("sync-let boundary error").expect("static string contains no null")
        });
        let message = sync_let_protect(sc, s7::s7_make_string(sc, message.as_ptr()));
        let info = sync_let_protect(sc, s7::s7_list(sc, 1, message.value));
        let error_args = sync_let_protect(
            sc,
            s7::s7_list(
                sc,
                2,
                s7::s7_make_symbol(sc, c"sync-web-error".as_ptr()),
                info.value,
            ),
        );
        let boundary = sync_let_protect(
            sc,
            s7::s7_list(
                sc,
                2,
                s7::s7_make_symbol(sc, c"%sync-let-error".as_ptr()),
                error_args.value,
            ),
        );
        let result = boundary.value;
        sync_let_unprotect(sc, boundary);
        sync_let_unprotect(sc, error_args);
        sync_let_unprotect(sc, info);
        sync_let_unprotect(sc, message);
        result
    }
}

fn primitive_s7_sync_let() -> Primitive {
    unsafe extern "C" fn code(
        sc: *mut s7::s7_scheme,
        args: s7::s7_pointer,
    ) -> s7::s7_pointer {
        unsafe {
            let names = s7::s7_car(args);
            let values = s7::s7_cadr(args);
            let body = s7::s7_caddr(args);
            if !s7::s7_is_proper_list(sc, names)
                || !s7::s7_is_proper_list(sc, values)
                || s7::s7_list_length(sc, names) != s7::s7_list_length(sc, values)
            {
                return sync_let_failure(sc, "sync-let binding names and values must be equal lists");
            }

            let environment = sync_let_env(sc);
            let mut names_cursor = names;
            let mut values_cursor = values;
            let mut seen = HashSet::new();
            while !s7::s7_is_null(sc, names_cursor) {
                let name = s7::s7_car(names_cursor);
                if !s7::s7_is_symbol(name) || !seen.insert(name as usize) {
                    sync_let_unprotect(sc, environment);
                    return sync_let_failure(sc, "sync-let binding names must be distinct symbols");
                }
                if s7::s7_is_syntax(s7::s7_let_ref(sc, s7::s7_rootlet(sc), name)) {
                    sync_let_unprotect(sc, environment);
                    return sync_let_failure(sc, "sync-let binding names cannot shadow syntax");
                }
                let value = match sync_let_copy(
                    sc,
                    s7::s7_car(values_cursor),
                    &mut HashSet::new(),
                    None,
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        sync_let_unprotect(sc, environment);
                        return sync_let_failure(sc, error.as_str());
                    }
                };
                s7::s7_varlet(sc, environment.value, name, value.value);
                sync_let_unprotect(sc, value);
                names_cursor = s7::s7_cdr(names_cursor);
                values_cursor = s7::s7_cdr(values_cursor);
            }

            if !s7::s7_is_byte_vector(body) {
                sync_let_unprotect(sc, environment);
                return sync_let_failure(sc, "sync-let body must be encoded code");
            }
            let bytes = (0..s7::s7_vector_length(body))
                .map(|index| s7::s7_byte_vector_ref(body, index))
                .collect::<Vec<_>>();
            let body = sync_let_protect(
                sc,
                s7::s7_make_string_with_length(
                    sc,
                    bytes.as_ptr() as *const c_char,
                    bytes.len() as s7::s7_int,
                ),
            );
            let wrapper_environment = sync_let_protect(
                sc,
                s7::s7_sublet(sc, s7::s7_rootlet(sc), s7::s7_nil(sc)),
            );
            let ok = sync_let_protect(
                sc,
                s7::s7_make_symbol(sc, c"%sync-let-ok".as_ptr()),
            );
            let error_tag = sync_let_protect(
                sc,
                s7::s7_make_symbol(sc, c"%sync-let-error".as_ptr()),
            );
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-body".as_ptr()),
                body.value,
            );
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-environment".as_ptr()),
                environment.value,
            );
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-eval".as_ptr()),
                s7::s7_let_ref(
                    sc,
                    s7::s7_rootlet(sc),
                    s7::s7_make_symbol(sc, c"eval-string".as_ptr()),
                ),
            );
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-ok".as_ptr()),
                ok.value,
            );
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-error".as_ptr()),
                error_tag.value,
            );
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-list".as_ptr()),
                s7::s7_let_ref(
                    sc,
                    s7::s7_rootlet(sc),
                    s7::s7_make_symbol(sc, c"list".as_ptr()),
                ),
            );
            let wrapper = CString::new(
                "(catch #t\
                   (lambda ()\
                     (%sync-let-list %sync-let-ok\
                       (%sync-let-eval %sync-let-body %sync-let-environment)))\
                   (lambda args (%sync-let-list %sync-let-error args)))",
            )
            .expect("Failed to construct sync-let wrapper");
            SESSIONS
                .write()
                .expect("Failed to acquire sessions lock")
                .get_mut(&(sc as usize))
                .expect("Session not found for sync-let")
                .sync_let_boundaries
                .push(environment.location);
            let tagged = sync_let_protect(
                sc,
                s7::s7_eval_c_string_with_environment(
                    sc,
                    wrapper.as_ptr(),
                    wrapper_environment.value,
                ),
            );
            SESSIONS
                .write()
                .expect("Failed to acquire sessions lock")
                .get_mut(&(sc as usize))
                .expect("Session not found for sync-let")
                .sync_let_boundaries
                .pop();
            sync_let_unprotect(sc, body);
            sync_let_unprotect(sc, wrapper_environment);
            if s7::s7_is_pair(tagged.value)
                && s7::s7_list_length(sc, tagged.value) == 2
                && s7::s7_car(tagged.value) == error_tag.value
            {
                let error_args = s7::s7_cadr(tagged.value);
                let copied_error = match sync_let_copy(
                    sc,
                    error_args,
                    &mut HashSet::new(),
                    None,
                ) {
                    Ok(copied) => copied,
                    Err(_) => {
                        sync_let_unprotect(sc, tagged);
                        sync_let_unprotect(sc, error_tag);
                        sync_let_unprotect(sc, ok);
                        sync_let_unprotect(sc, environment);
                        return sync_let_failure(sc, "sync-let error contained a non-inert value");
                    }
                };
                if !s7::s7_is_proper_list(sc, copied_error.value)
                    || s7::s7_list_length(sc, copied_error.value) != 2
                {
                    sync_let_unprotect(sc, copied_error);
                    sync_let_unprotect(sc, tagged);
                    sync_let_unprotect(sc, error_tag);
                    sync_let_unprotect(sc, ok);
                    sync_let_unprotect(sc, environment);
                    return sync_let_failure(sc, "sync-let error has an invalid boundary shape");
                }
                s7::s7_set_car(s7::s7_cdr(tagged.value), copied_error.value);
                let result = tagged.value;
                sync_let_unprotect(sc, copied_error);
                sync_let_unprotect(sc, tagged);
                sync_let_unprotect(sc, error_tag);
                sync_let_unprotect(sc, ok);
                sync_let_unprotect(sc, environment);
                return result;
            }

            if !s7::s7_is_pair(tagged.value)
                || s7::s7_list_length(sc, tagged.value) != 2
                || s7::s7_car(tagged.value) != ok.value
            {
                sync_let_unprotect(sc, tagged);
                sync_let_unprotect(sc, error_tag);
                sync_let_unprotect(sc, ok);
                sync_let_unprotect(sc, environment);
                return sync_let_failure(sc, "sync-let evaluation returned an invalid boundary result");
            }
            let copied = match sync_let_copy(
                sc,
                s7::s7_cadr(tagged.value),
                &mut HashSet::new(),
                None,
            ) {
                Ok(copied) => copied,
                Err(error) => {
                    sync_let_unprotect(sc, tagged);
                    sync_let_unprotect(sc, error_tag);
                    sync_let_unprotect(sc, ok);
                    sync_let_unprotect(sc, environment);
                    return sync_let_failure(sc, error.as_str());
                }
            };
            s7::s7_set_car(s7::s7_cdr(tagged.value), copied.value);
            let result = tagged.value;
            sync_let_unprotect(sc, copied);
            sync_let_unprotect(sc, tagged);
            sync_let_unprotect(sc, error_tag);
            sync_let_unprotect(sc, ok);
            sync_let_unprotect(sc, environment);
            result
        }
    }

    Primitive::new(
        code,
        c"%sync-let",
        c"internal sync-let copied-data sandbox evaluator",
        3,
        0,
        false,
    )
}

fn primitive_s7_sync_let_return() -> Primitive {
    unsafe extern "C" fn code(
        sc: *mut s7::s7_scheme,
        args: s7::s7_pointer,
    ) -> s7::s7_pointer {
        unsafe {
            let boundary = s7::s7_car(args);
            if !s7::s7_is_proper_list(sc, boundary)
                || s7::s7_list_length(sc, boundary) != 2
            {
                return sync_error(sc, "sync-let returned an invalid internal boundary shape");
            }
            let tag = s7::s7_car(boundary);
            let ok = s7::s7_make_symbol(sc, c"%sync-let-ok".as_ptr());
            let error = s7::s7_make_symbol(sc, c"%sync-let-error".as_ptr());
            if tag == ok {
                return s7::s7_cadr(boundary);
            }
            if tag != error {
                return sync_error(sc, "sync-let returned an invalid internal boundary tag");
            }
            let error_args = s7::s7_cadr(boundary);
            if !s7::s7_is_proper_list(sc, error_args)
                || s7::s7_list_length(sc, error_args) != 2
            {
                return sync_error(sc, "sync-let error has an invalid boundary shape");
            }
            s7::s7_error(sc, s7::s7_car(error_args), s7::s7_cadr(error_args))
        }
    }

    Primitive::new(
        code,
        c"%sync-let-return",
        c"internal sync-let boundary result transfer",
        1,
        0,
        false,
    )
}

unsafe fn active_shared_boundary(sc: *mut s7::s7_scheme) -> Option<s7::s7_pointer> {
    unsafe {
        let location = SESSIONS
            .read()
            .expect("Failed to acquire sessions lock")
            .get(&(sc as usize))
            .and_then(|session| {
                if serialization_trace_active(sc) {
                    session.serialization_env_loc
                } else {
                    session.sync_let_boundaries.last().copied()
                }
            })?;
        Some(s7::s7_gc_protected_at(sc, location))
    }
}

unsafe fn shared_boundary_contains(
    sc: *mut s7::s7_scheme,
    boundary: s7::s7_pointer,
    procedure: s7::s7_pointer,
) -> bool {
    unsafe {
        let mut environment = s7::s7_funclet(sc, procedure);
        while !environment.is_null()
            && s7::s7_is_let(environment)
            && environment != boundary
        {
            environment = s7::s7_outlet(sc, environment);
        }
        environment == boundary
    }
}

fn primitive_s7_sync_safe_setter() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let target = s7::s7_car(args);
            if !s7::s7_is_procedure(target) {
                return s7::s7_f(sc);
            }
            let Some(boundary) = active_shared_boundary(sc) else {
                return s7::s7_f(sc);
            };
            if !shared_boundary_contains(sc, boundary, target) {
                return s7::s7_f(sc);
            }
            s7::s7_setter(sc, target)
        }
    }

    Primitive::new(
        code,
        c"%sync-safe-setter",
        c"internal getter for shared-computation procedure setters",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_safe_setter_set() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let target = s7::s7_car(args);
            let setter = s7::s7_cadr(args);
            if !s7::s7_is_procedure(target) || !s7::s7_is_procedure(setter) {
                return sync_error(sc, "safe setter requires two procedures");
            }
            let Some(boundary) = active_shared_boundary(sc) else {
                return sync_error(sc, "safe setter shared boundary is unavailable");
            };
            if !shared_boundary_contains(sc, boundary, target)
                || !shared_boundary_contains(sc, boundary, setter)
            {
                return sync_error(
                    sc,
                    "safe setter rejects procedures outside the active shared boundary",
                );
            }
            s7::s7_set_setter(sc, target, setter)
        }
    }

    Primitive::new(
        code,
        c"%sync-safe-setter-set!",
        c"internal setter for shared-computation procedure setters",
        2,
        0,
        false,
    )
}

fn primitive_s7_sync_let_active() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, _args: s7::s7_pointer) -> s7::s7_pointer {
        let active = SESSIONS
            .read()
            .expect("Failed to acquire sessions lock")
            .get(&(sc as usize))
            .is_some_and(|session| !session.sync_let_boundaries.is_empty());
        unsafe { s7::s7_make_boolean(sc, active) }
    }

    Primitive::new(
        code,
        c"sync-let-active?",
        c"(sync-let-active?) report whether shared computation is active",
        0,
        0,
        false,
    )
}

fn primitive_s7_sync_let_eval() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let location = SESSIONS
                .read()
                .expect("Failed to acquire sessions lock")
                .get(&(sc as usize))
                .and_then(|session| session.sync_let_boundaries.last().copied());
            let Some(location) = location else {
                return sync_error(sc, "sync-let-eval is available only inside sync-let");
            };
            let source = obj2str(sc, s7::s7_car(args));
            let source = match CString::new(source) {
                Ok(source) => source,
                Err(_) => return sync_error(sc, "sync-let-eval expression contains a null byte"),
            };
            s7::s7_eval_c_string_with_environment(
                sc,
                source.as_ptr(),
                s7::s7_gc_protected_at(sc, location),
            )
        }
    }

    Primitive::new(
        code,
        c"sync-let-eval",
        c"(sync-let-eval expression) evaluate copied code in the current sync-let",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_eval() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let eval_env = s7::s7_gc_protect_via_stack(sc, s7::s7_curlet(sc));
            let expression = s7::s7_gc_protect_via_stack(sc, s7::s7_car(args));
            if !sync_is_node(expression) {
                let value = expression;
                s7::s7_gc_unprotect_via_stack(sc, expression);
                s7::s7_gc_unprotect_via_stack(sc, eval_env);
                return s7::s7_wrong_type_arg_error(
                    sc, c"sync-eval".as_ptr(), 1, value, c"a sync-node".as_ptr(),
                );
            }
            let header = s7::s7_gc_protect_via_stack(
                sc,
                sync_cxr(
                    sc,
                    s7::s7_list(sc, 1, expression),
                    c"sync-eval",
                    true,
                    |children| children.0,
                ),
            );
            if !s7::s7_is_byte_vector(header) {
                s7::s7_gc_unprotect_via_stack(sc, header);
                s7::s7_gc_unprotect_via_stack(sc, expression);
                s7::s7_gc_unprotect_via_stack(sc, eval_env);
                return sync_error(
                    sc,
                    "sync-eval first argument should be a sync-node with a byte-vector header",
                );
            }
            let mut bytes = vec![39];
            for index in 0..s7::s7_vector_length(header) {
                bytes.push(s7::s7_byte_vector_ref(header, index));
            }
            bytes.push(0);
            let code = match CString::from_vec_with_nul(bytes) {
                Ok(code) => code,
                Err(_) => {
                    s7::s7_gc_unprotect_via_stack(sc, header);
                    s7::s7_gc_unprotect_via_stack(sc, expression);
                    s7::s7_gc_unprotect_via_stack(sc, eval_env);
                    return s7::s7_error(
                        sc,
                        s7::s7_make_symbol(sc, c"encoding-error".as_ptr()),
                        s7::s7_list(
                            sc,
                            1,
                            s7::s7_make_string(sc, c"Byte vector string is malformed".as_ptr()),
                        ),
                    );
                }
            };
            let loader_expression = s7::s7_gc_protect_via_stack(
                sc,
                s7::s7_eval_c_string_with_environment(sc, code.as_ptr(), eval_env),
            );
            let loader = s7::s7_gc_protect_via_stack(
                sc,
                s7::s7_eval(sc, loader_expression, eval_env),
            );
            let result = s7::s7_gc_protect_via_stack(
                sc,
                s7::s7_apply_function(sc, loader, s7::s7_list(sc, 1, expression)),
            );
            s7::s7_gc_unprotect_via_stack(sc, result);
            s7::s7_gc_unprotect_via_stack(sc, loader);
            s7::s7_gc_unprotect_via_stack(sc, loader_expression);
            s7::s7_gc_unprotect_via_stack(sc, header);
            s7::s7_gc_unprotect_via_stack(sc, expression);
            s7::s7_gc_unprotect_via_stack(sc, eval_env);
            result
        }
    }

    Primitive::new(
        code,
        c"sync-eval",
        c"(sync-eval node) load a sync-node in the current environment",
        1,
        0,
        false,
    )
}

fn primitive_s7_sync_http() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            mark_external_called(sc);

            let vec2s7 = |vector: Vec<u8>| {
                let bv = s7::s7_make_byte_vector(sc, vector.len() as i64, 1, std::ptr::null_mut());
                for i in 0..vector.len() {
                    s7::s7_byte_vector_set(bv, i as i64, vector[i]);
                }
                bv
            };

            let method = obj2str(sc, s7::s7_car(args));
            let url = obj2str(sc, s7::s7_cadr(args));

            let body = if s7::s7_list_length(sc, args) >= 3 {
                obj2str(sc, s7::s7_caddr(args))
            } else {
                String::from("")
            };

            if SESSIONS
                .read()
                .expect("Failed to acquire session lock")
                .get(&(sc as usize))
                .and_then(|session| session.scenario.as_ref())
                .is_some()
            {
                return sync_error(sc, "sync-http is not supported by the scenario harness");
            }

            let cache_mutex = {
                let session = SESSIONS.read().expect("Failed to acquire sessions lock");
                session
                    .get(&(sc as usize))
                    .expect("Session ID not found in active sessions")
                    .cache
                    .clone()
            };

            let mut cache = cache_mutex
                .lock()
                .expect("Failed to acquire cache mutex lock");

            let key = (method.clone(), url.clone(), body.as_bytes().to_vec());

            match cache.get(&key) {
                Some(bytes) => {
                    debug!("Cache hit on key {:?}", key);
                    vec2s7(bytes.to_vec())
                }
                None => {
                    let result = tokio::task::block_in_place(move || {
                        tokio::runtime::Handle::current().block_on(async move {
                            match method.to_lowercase() {
                                method if method == "get" => {
                                    JOURNAL
                                        .client
                                        .get(&url[1..url.len() - 1])
                                        .send()
                                        .await?
                                        .bytes()
                                        .await
                                }
                                method if method == "post" => {
                                    JOURNAL
                                        .client
                                        .post(&url[1..url.len() - 1])
                                        .body(String::from(&body[1..body.len() - 1]))
                                        .send()
                                        .await?
                                        .bytes()
                                        .await
                                }
                                _ => {
                                    panic!("Unsupported HTTP method")
                                }
                            }
                        })
                    });

                    match result {
                        Ok(vector) => {
                            cache.insert(key, vector.to_vec());
                            vec2s7(vector.to_vec())
                        }
                        Err(_) => {
                            sync_error(sc, "Journal is unable to fulfill HTTP request (sync-http)")
                        }
                    }
                }
            }
        }
    }

    Primitive::new(
        code,
        c"sync-http",
        c"(sync-http method url . data) make an http request where method is 'get or 'post",
        2,
        2,
        false,
    )
}

fn primitive_s7_sync_remote() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            mark_external_called(sc);

            let vec2s7 = |mut vector: Vec<u8>| {
                vector.insert(0, 39); // add quote character so that it evaluates correctly
                vector.push(0);
                let c_string = CString::from_vec_with_nul(vector)
                    .expect("Failed to create C string from vector");
                s7::s7_eval_c_string(sc, c_string.as_ptr())
            };

            let url = obj2str(sc, s7::s7_car(args));

            let body = obj2str(sc, s7::s7_cadr(args));

            let cache_mutex = {
                let session = SESSIONS.read().expect("Failed to acquire session lock");
                session
                    .get(&(sc as usize))
                    .expect("Failed to get session from map")
                    .cache
                    .clone()
            };

            let mut cache = cache_mutex.lock().expect("Failed to acquire cache lock");

            let key = (String::from("post"), url.clone(), body.as_bytes().to_vec());

            match cache.get(&key) {
                Some(bytes) => {
                    debug!("Cache hit on key {:?}", key);
                    vec2s7(bytes.to_vec())
                }
                None => {
                    let scenario = {
                        let sessions = SESSIONS.read().expect("Failed to acquire session lock");
                        let session = sessions
                            .get(&(sc as usize))
                            .expect("Failed to get session from map");
                        session
                            .scenario
                            .as_ref()
                            .map(|scenario| (scenario.clone(), session.record))
                    };
                    let result: Result<Vec<u8>, String> = if let Some((scenario, source)) = scenario
                    {
                        scenario.transport.remote(
                            scenario.action,
                            source,
                            url[1..url.len() - 1].to_string(),
                            body,
                        )
                    } else {
                        tokio::task::block_in_place(move || {
                        tokio::runtime::Handle::current().block_on(async move {
                            JOURNAL
                                .client
                                .post(&url[1..url.len() - 1])
                                .body(body)
                                .send()
                                    .await
                                    .map_err(|error| error.to_string())?
                                .bytes()
                                .await
                                    .map(|bytes| bytes.to_vec())
                                    .map_err(|error| error.to_string())
                        })
                        })
                    };

                    match result {
                        Ok(bytes) => {
                            cache.insert(key, bytes.to_vec());
                            vec2s7(bytes)
                        }
                        Err(_) => {
                            sync_error(sc, "Journal is unable to query remote peer (sync-remote)")
                        }
                    }
                }
            }
        }
    }

    Primitive::new(
        code,
        c"sync-remote",
        c"(sync-remote url data) make a post http request with the data payload)",
        2,
        0,
        false,
    )
}
unsafe fn string_to_s7(sc: *mut s7::s7_scheme, string: &str) -> s7::s7_pointer {
    unsafe {
        let c_string = CString::new(string).expect("Failed to create CString from string");
        let s7_string = s7::s7_make_string(sc, c_string.as_ptr());
        s7::s7_object_to_string(sc, s7_string, false)
    }
}

unsafe fn sync_heap_make(word: Word) -> *mut libc::c_void {
    unsafe {
        let ptr = libc::malloc(SIZE);
        let array: &mut [u8] = std::slice::from_raw_parts_mut(ptr as *mut u8, SIZE);
        for i in 0..SIZE {
            array[i] = word[i] as u8;
        }
        ptr
    }
}

pub(crate) unsafe fn sync_heap_read(ptr: *mut libc::c_void) -> Word {
    unsafe {
        std::slice::from_raw_parts_mut(ptr as *mut u8, SIZE)
            .try_into()
            .expect("Failed to convert slice to Word array")
    }
}

unsafe fn sync_heap_free(ptr: *mut libc::c_void) {
    unsafe {
        libc::free(ptr);
    }
}

pub(crate) unsafe fn sync_is_node(obj: s7::s7_pointer) -> bool {
    unsafe { s7::s7_is_c_object(obj) && s7::s7_c_object_type(obj) == SYNC_NODE_TAG }
}

unsafe fn sync_cxr(
    sc: *mut s7::s7_scheme,
    args: s7::s7_pointer,
    name: &CStr,
    left_side: bool,
    selector: fn((Word, Word)) -> Word,
) -> s7::s7_pointer {
    unsafe {
        let node = s7::s7_car(args);
        let word = sync_heap_read(s7::s7_c_object_value(node));
        let persistor = session_persistor_for(sc);

        let child_return = |word| {
            let node_return = |word| s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(word));

            let vector_return = |vector: Vec<u8>| {
                let bv = s7::s7_make_byte_vector(sc, vector.len() as i64, 1, std::ptr::null_mut());
                for i in 0..vector.len() {
                    s7::s7_byte_vector_set(bv, i as i64, vector[i]);
                }
                bv
            };

            if word == NULL {
                return node_return(word);
            }

            match resolve_node_with(&persistor, word) {
                Some(ResolvedNode::Branch((left, right, digest), ResolveSource::Global)) => {
                    persistor
                        .branch_set(left, right, digest)
                        .expect("Failed to add branch to session persistor");
                    node_return(word)
                }
                Some(ResolvedNode::Branch(_, _)) => node_return(word),
                Some(ResolvedNode::Leaf(content, ResolveSource::Global)) => {
                    persistor
                        .leaf_set(content.clone())
                        .expect("Failed to add leaf to session persistor");
                    vector_return(content)
                }
                Some(ResolvedNode::Leaf(content, _)) => vector_return(content),
                Some(ResolvedNode::Stump(digest, ResolveSource::Global)) => {
                    persistor
                        .stump_set(digest)
                        .expect("Failed to add stump to session persistor");
                    node_return(word)
                }
                Some(ResolvedNode::Stump(_, _)) => node_return(word),
                None => sync_error(
                    sc,
                    format!(
                        "Cannot retrieve items for node that is not a sync-pair ({})",
                        name.to_string_lossy()
                    )
                    .as_str(),
                ),
            }
        };

        match sync_is_node(node) {
            true => match resolve_branch_with(&persistor, word) {
                Some(((left, right, _), _)) => {
                    let child = selector((left, right));
                    serialization_trace_child(sc, word, child, left_side);
                    child_return(child)
                }
                None => sync_error(
                    sc,
                    format!(
                        "Journal cannot retrieve leaf byte-vector ({})",
                        name.to_string_lossy()
                    )
                    .as_str(),
                ),
            },
            false => {
                s7::s7_wrong_type_arg_error(sc, name.as_ptr(), 1, node, c"a sync-node".as_ptr())
            }
        }
    }
}

unsafe fn sync_digest(sc: *mut s7::s7_scheme, word: Word) -> Result<Word, String> {
    let persistor = session_persistor_for(sc);

    if word == NULL {
        Ok(NULL)
    } else {
        match resolve_node_with(&persistor, word) {
            Some(ResolvedNode::Branch((_, _, digest), _)) => Ok(digest),
            Some(ResolvedNode::Leaf(_, _)) => Ok(word),
            Some(ResolvedNode::Stump(digest, _)) => Ok(digest),
            None => Err("Digest not found in persistor".to_string()),
        }
    }
}

unsafe fn sync_branch_children(sc: *mut s7::s7_scheme, word: Word) -> Result<(Word, Word), String> {
    let persistor = session_persistor_for(sc);

    if let Some(((left, right, _), _)) = resolve_branch_with(&persistor, word) {
        Ok((left, right))
    } else {
        Err("Node is not a sync-pair".to_string())
    }
}

pub(crate) fn session_persistor_for(sc: *mut s7::s7_scheme) -> MemoryPersistor {
    let session = SESSIONS.read().expect("Failed to acquire SESSIONS lock");
    session
        .get(&(sc as usize))
        .expect("Session not found for given context")
        .persistor
        .clone()
}

pub(crate) unsafe fn session_serialization_query_get(
    sc: *mut s7::s7_scheme,
    source: &str,
) -> Option<s7::s7_pointer> {
    unsafe {
        SESSIONS
            .read()
            .expect("Failed to acquire SESSIONS lock")
            .get(&(sc as usize))
            .and_then(|session| session.serialization_query_locs.get(source))
            .map(|location| s7::s7_gc_protected_at(sc, *location))
    }
}

pub(crate) unsafe fn session_serialization_query_store(
    sc: *mut s7::s7_scheme,
    source: String,
    expression: s7::s7_pointer,
) -> s7::s7_pointer {
    unsafe {
        let location = s7::s7_gc_protect(sc, expression);
        SESSIONS
            .write()
            .expect("Failed to acquire SESSIONS lock")
            .get_mut(&(sc as usize))
            .expect("Session not found for given context")
            .serialization_query_locs
            .insert(source, location);
        expression
    }
}

pub(crate) unsafe fn serialization_query_environment(
    sc: *mut s7::s7_scheme,
) -> (s7::s7_pointer, s7::s7_int) {
    unsafe {
        let existing = SESSIONS
            .read()
            .expect("Failed to acquire SESSIONS lock")
            .get(&(sc as usize))
            .and_then(|session| session.serialization_env_loc);
        let base = match existing {
            Some(location) => s7::s7_gc_protected_at(sc, location),
            None => {
                let environment = sync_let_env(sc);
                SESSIONS
                    .write()
                    .expect("Failed to acquire SESSIONS lock")
                    .get_mut(&(sc as usize))
                    .expect("Session not found for given context")
                    .serialization_env_loc = Some(environment.location);
                environment.value
            }
        };
        let environment = s7::s7_sublet(sc, base, s7::s7_nil(sc));
        let environment_location = s7::s7_gc_protect(sc, environment);
        for name in SYNC_LET_ALLOWED
            .iter()
            .chain(SERIALIZATION_QUERY_ADDITIONAL.iter())
        {
            let name = CString::new(*name).expect("serialization capability contains a null byte");
            let symbol = s7::s7_make_symbol(sc, name.as_ptr());
            s7::s7_define(
                sc,
                environment,
                symbol,
                s7::s7_let_ref(sc, s7::s7_rootlet(sc), symbol),
            );
        }
        s7::s7_define(
            sc,
            environment,
            s7::s7_make_symbol(sc, c"setter".as_ptr()),
            s7::s7_let_ref(
                sc,
                s7::s7_rootlet(sc),
                s7::s7_make_symbol(sc, c"%sync-safe-setter".as_ptr()),
            ),
        );
        for name in [
            c"sync-serialize",
            c"sync-deserialize",
            c"sync-let-active?",
            c"sync-let-eval",
        ] {
            s7::s7_define(
                sc,
                environment,
                s7::s7_make_symbol(sc, name.as_ptr()),
                s7::s7_undefined(sc),
            );
        }
        (environment, environment_location)
    }
}

pub(crate) unsafe fn serialization_error_copy(
    sc: *mut s7::s7_scheme,
    error_args: s7::s7_pointer,
) -> Option<(s7::s7_pointer, s7::s7_pointer)> {
    unsafe {
        let copied = sync_let_copy(sc, error_args, &mut HashSet::new(), None).ok()?;
        if !s7::s7_is_proper_list(sc, copied.value)
            || s7::s7_list_length(sc, copied.value) != 2
        {
            sync_let_unprotect(sc, copied);
            return None;
        }
        let error_type = s7::s7_gc_protect_via_stack(sc, s7::s7_car(copied.value));
        let error_info = s7::s7_gc_protect_via_stack(sc, s7::s7_cadr(copied.value));
        sync_let_unprotect(sc, copied);
        Some((error_type, error_info))
    }
}

struct SyncLetMask {
    sc: *mut s7::s7_scheme,
    environment: s7::s7_pointer,
}

unsafe extern "C" fn sync_let_mask_symbol(
    name: *const c_char,
    data: *mut c_void,
) -> bool {
    unsafe {
        let mask = &mut *(data as *mut SyncLetMask);
        let symbol = s7::s7_make_symbol(mask.sc, name);
        s7::s7_define(
            mask.sc,
            mask.environment,
            symbol,
            s7::s7_undefined(mask.sc),
        );
        false
    }
}

unsafe fn sync_let_env(sc: *mut s7::s7_scheme) -> SyncLetProtected {
    unsafe {
        let environment = sync_let_protect(
            sc,
            s7::s7_sublet(sc, s7::s7_rootlet(sc), s7::s7_nil(sc)),
        );
        let capabilities = SYNC_LET_ALLOWED
            .iter()
            .map(|name| {
                let name = CString::new(*name)
                    .expect("sync-let capability contains a null byte");
                let symbol = sync_let_protect(sc, s7::s7_make_symbol(sc, name.as_ptr()));
                let mut value = s7::s7_let_ref(sc, s7::s7_rootlet(sc), symbol.value);
                if value == s7::s7_undefined(sc) {
                    value = s7::s7_symbol_value(sc, symbol.value);
                }
                (symbol, sync_let_protect(sc, value))
            })
            .collect::<Vec<_>>();
        let mut mask = SyncLetMask {
            sc,
            environment: environment.value,
        };
        s7::s7_for_each_symbol(
            sc,
            Some(sync_let_mask_symbol),
            &mut mask as *mut SyncLetMask as *mut c_void,
        );
        for (symbol, value) in capabilities {
            s7::s7_define(sc, environment.value, symbol.value, value.value);
            sync_let_unprotect(sc, value);
            sync_let_unprotect(sc, symbol);
        }
        s7::s7_define(
            sc,
            environment.value,
            s7::s7_make_symbol(sc, c"setter".as_ptr()),
            s7::s7_let_ref(
                sc,
                s7::s7_rootlet(sc),
                s7::s7_make_symbol(sc, c"%sync-safe-setter".as_ptr()),
            ),
        );
        environment
    }
}
